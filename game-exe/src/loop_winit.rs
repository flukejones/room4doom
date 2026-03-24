//! winit `ApplicationHandler`-based game loop.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use doom_ui::{Finale, GameMenu, Intermission, Messages, Statusbar};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{ConfigTraits, SubsystemTrait};
use input::InputState;
use log::{info, warn};
use render_backend::{DisplayBackend, RenderTarget};
use render_common::{GameRenderer, STBAR_HEIGHT};
use software3d::DebugDrawOptions;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::CLIOptions;
use crate::cheats::Cheats;
use crate::d_main::{
    d_display, input_responder, load_voxels, run_game_tic, set_lookdirs, set_lookdirs_hires, update_sound
};
use crate::timestep::TimeStep;

/// Create the appropriate display backend for the active feature.
#[cfg(feature = "display-pixels")]
fn new_display_backend(window: Arc<Window>, vsync: bool) -> DisplayBackend {
    DisplayBackend::new_pixels(window, vsync)
}

#[cfg(all(feature = "display-softbuffer", not(feature = "display-pixels")))]
fn new_display_backend(window: Arc<Window>, _vsync: bool) -> DisplayBackend {
    DisplayBackend::new_softbuffer(window)
}

/// All game state owned by the winit event loop.
pub struct DoomApp {
    game: Game,
    input: InputState,
    cheats: Cheats,
    timestep: TimeStep,
    render_backend: Option<RenderTarget>,
    menu: Option<GameMenu>,
    machines: GameSubsystem<Intermission, Statusbar, Messages, Finale>,
    options: CLIOptions,
    debug_draw: DebugDrawOptions,
    window: Option<Arc<Window>>,
    /// Frame interval for vsync pacing. `None` = uncapped.
    frame_interval: Option<Duration>,
    /// Target instant for the next frame when vsync is active.
    next_frame: Instant,
    voxel_manager: Option<std::sync::Arc<pic_data::VoxelManager>>,
    user_config: crate::config::UserConfig,
}

impl DoomApp {
    /// Create the app with game state ready. Window is created on `resumed`.
    pub fn new(
        game: Game,
        input: InputState,
        options: CLIOptions,
        user_config: crate::config::UserConfig,
    ) -> Self {
        let debug_draw = options.debug_draw();
        let machines = GameSubsystem {
            statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
            intermission: Intermission::new(game.game_type.mode, &game.wad_data, &game.umapinfo),
            hud_msgs: Messages::new(&game.wad_data),
            finale: Finale::new(&game.wad_data),
        };
        let voxel_manager = load_voxels(
            &options,
            &game.wad_data,
            game.game_type.mode,
            game.pic_data.pwad_sprite_overrides(),
        );
        Self {
            game,
            input,
            cheats: Cheats::new(),
            timestep: TimeStep::new(),
            render_backend: None,
            menu: None,
            machines,
            options,
            debug_draw,
            window: None,
            frame_interval: None,
            next_frame: Instant::now(),
            voxel_manager,
            user_config,
        }
    }
}

impl ApplicationHandler for DoomApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let req_w = self.options.width;
        let req_h = self.options.height;

        let mut attrs = Window::default_attributes()
            .with_title("ROOM4DOOM")
            .with_inner_size(winit::dpi::PhysicalSize::new(req_w, req_h));

        let wm = self
            .options
            .window_mode
            .unwrap_or(crate::config::WindowMode::Windowed);
        match wm {
            crate::config::WindowMode::Windowed => {}
            crate::config::WindowMode::Borderless => {
                attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            crate::config::WindowMode::Exclusive => {
                let req_hz = self.options.refresh_rate as u32 * 1000;
                let monitor = event_loop
                    .primary_monitor()
                    .or_else(|| event_loop.available_monitors().next());
                let mode = monitor.and_then(|mon| {
                    let modes: Vec<_> = mon.video_modes().collect();
                    if modes.is_empty() {
                        return None;
                    }
                    let best = modes
                        .iter()
                        .min_by_key(|m| {
                            let dw = m.size().width as i64 - req_w as i64;
                            let dh = m.size().height as i64 - req_h as i64;
                            let dist = dw * dw + dh * dh;
                            let hz_dist = if req_hz > 0 {
                                let dz = m.refresh_rate_millihertz() as i64 - req_hz as i64;
                                dz * dz
                            } else {
                                -(m.refresh_rate_millihertz() as i64)
                            };
                            (dist, hz_dist)
                        })
                        .unwrap();
                    let sz = best.size();
                    let hz = best.refresh_rate_millihertz();
                    if sz.width != req_w || sz.height != req_h || (req_hz > 0 && hz != req_hz) {
                        warn!(
                            "No exact match for {}x{}@{}Hz, using {}x{}@{:.1}Hz. Available:",
                            req_w,
                            req_h,
                            if req_hz > 0 { req_hz / 1000 } else { 0 },
                            sz.width,
                            sz.height,
                            hz as f32 / 1000.0
                        );
                        let mut by_res: BTreeMap<(u32, u32), Vec<u32>> =
                            BTreeMap::new();
                        for m in &modes {
                            let s = m.size();
                            let rates = by_res.entry((s.width, s.height)).or_default();
                            let r = m.refresh_rate_millihertz();
                            if !rates.contains(&r) {
                                rates.push(r);
                            }
                        }
                        for ((w, h), rates) in &by_res {
                            let hz: Vec<_> = rates
                                .iter()
                                .map(|r| format!("{:.1}", *r as f32 / 1000.0))
                                .collect();
                            info!("  {}x{} @ {}Hz", w, h, hz.join(", "));
                        }
                    } else {
                        info!(
                            "Exclusive fullscreen: {}x{}@{:.1}Hz",
                            sz.width,
                            sz.height,
                            hz as f32 / 1000.0
                        );
                    }
                    Some(best.clone())
                });
                let fullscreen = mode
                    .map(|m| winit::window::Fullscreen::Exclusive(m))
                    .unwrap_or(winit::window::Fullscreen::Borderless(None));
                attrs = attrs.with_fullscreen(Some(fullscreen));
            }
        }
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create winit window"),
        );

        window.set_cursor_visible(false);
        window
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(winit::window::CursorGrabMode::Locked))
            .expect("failed to grab cursor");

        let vsync = self.options.vsync.unwrap_or(true);
        // With the pixels (wgpu) backend, vsync is handled by the GPU present
        // mode — wgpu blocks in render() until vblank. Adding a winit-level
        // frame interval on top double-throttles and can cause white frames.
        let use_winit_interval = vsync && !cfg!(feature = "display-pixels");
        if use_winit_interval {
            let monitor = window
                .current_monitor()
                .or_else(|| event_loop.primary_monitor())
                .or_else(|| event_loop.available_monitors().next());
            let refresh_mhz = monitor
                .and_then(|m| m.video_modes().map(|v| v.refresh_rate_millihertz()).max())
                .unwrap_or(60_000);
            let interval = Duration::from_nanos(1_000_000_000_000 / refresh_mhz as u64);
            info!(
                "vsync: frame interval {:.2}ms ({:.1}Hz)",
                interval.as_secs_f64() * 1000.0,
                refresh_mhz as f64 / 1000.0
            );
            self.frame_interval = Some(interval);
            self.next_frame = Instant::now();
        } else if vsync {
            info!("vsync: GPU present mode (pixels/wgpu)");
            self.frame_interval = None;
        } else {
            info!("vsync disabled, uncapped frame rate");
            self.frame_interval = None;
        }

        let display = new_display_backend(window.clone(), self.options.vsync.unwrap_or(true));

        set_lookdirs(&self.options);
        let mut render_backend = RenderTarget::new(
            self.options.hi_res.unwrap_or(true),
            self.options.dev_parm,
            &self.debug_draw,
            display,
            self.options.rendering.unwrap_or_default().into(),
        );
        if self.user_config.hud_size == 1 {
            render_backend.set_statusbar_height(STBAR_HEIGHT);
        }
        self.input
            .events
            .set_mouse_scale((self.user_config.mouse_sensitivity, 1));
        self.input.events.set_invert_y(self.user_config.invert_y);
        if let Some(ref vm) = self.voxel_manager {
            render_backend.set_voxel_manager(vm.clone());
        }
        let mut menu = GameMenu::new(
            self.game.game_type.mode,
            &self.game.wad_data,
            render_backend.buffer_size().width(),
        );
        menu.init(&self.game);

        if let Some(name) = self.options.demo.clone() {
            self.game.start_demo(name);
        } else if self.options.episode.is_none() && self.options.map.is_none() {
            self.game.start_title();
        }
        info!("Started title sequence");

        self.render_backend = Some(render_backend);
        self.menu = Some(menu);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event,
                ..
            } => {
                if let PhysicalKey::Code(wk) = event.physical_key {
                    if let Some(kc) = input::winit_keycode_to_keycode(wk) {
                        match event.state {
                            ElementState::Pressed if !event.repeat => {
                                let menu = self.menu.as_mut().expect("menu not initialized");
                                let consumed = input_responder(
                                    kc,
                                    &mut self.game,
                                    menu,
                                    &mut self.machines,
                                    &mut self.cheats,
                                );
                                if consumed {
                                    self.input.events.unset_kb(kc);
                                } else {
                                    self.input.events.set_kb(kc);
                                }
                            }
                            ElementState::Released => {
                                self.input.events.unset_kb(kc);
                            }
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button,
                ..
            } => {
                if let Some(mb) = input::winit_mousebutton_to_mousebtn(button) {
                    match state {
                        ElementState::Pressed => self.input.events.set_mb(mb),
                        ElementState::Released => self.input.events.unset_mb(mb),
                    }
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(window) = &self.window {
                    let display = new_display_backend(window.clone(), self.user_config.vsync);
                    set_lookdirs_hires(self.user_config.hi_res);
                    let prev_state = self.menu.as_ref().map(|m| m.save_state());
                    let mut rt = RenderTarget::new(
                        self.user_config.hi_res,
                        self.options.dev_parm,
                        &self.debug_draw,
                        display,
                        self.user_config.renderer.into(),
                    );
                    if self.user_config.hud_size == 1 {
                        rt.set_statusbar_height(STBAR_HEIGHT);
                    }
                    if self.user_config.voxels {
                        if let Some(ref vm) = self.voxel_manager {
                            rt.set_voxel_manager(vm.clone());
                        }
                    }
                    let mut menu = GameMenu::new(
                        self.game.game_type.mode,
                        &self.game.wad_data,
                        rt.buffer_size().width(),
                    );
                    menu.init(&self.game);
                    if let Some(state) = prev_state {
                        menu.restore_state(state);
                    }
                    self.render_backend = Some(rt);
                    self.menu = Some(menu);
                }
            }
            WindowEvent::RedrawRequested => {
                if !self.game.running() {
                    event_loop.exit();
                    return;
                }

                {
                    let rt = self
                        .render_backend
                        .as_mut()
                        .expect("render target not initialized");
                    let menu = self.menu.as_mut().expect("menu not initialized");

                    self.timestep.run_this(|| {
                        run_game_tic(&mut self.game, &mut self.input, menu, &mut self.machines);
                    });

                    let frac = if self.user_config.frame_interpolation {
                        self.timestep.frac()
                    } else {
                        1.0
                    };
                    update_sound(&self.game);
                    d_display(rt, menu, &mut self.machines, &mut self.game, frac);
                }

                if self.game.is_config_dirty() {
                    let old = self.user_config.clone();
                    self.user_config
                        .apply_config_array(&self.game.config_snapshot());
                    if let Some(rt) = self.render_backend.as_ref() {
                        let (w, h) = rt.window_size();
                        self.user_config.width = w;
                        self.user_config.height = h;
                    }
                    self.user_config.write();
                    self.game.clear_config_dirty();

                    if old.crt_gamma != self.user_config.crt_gamma {
                        self.game.pic_data.set_crt_gamma(self.user_config.crt_gamma);
                    }

                    if old.window_mode != self.user_config.window_mode {
                        if let Some(rt) = self.render_backend.as_mut() {
                            let mode = self.game.config_snapshot()
                                [gamestate_traits::ConfigKey::WindowMode as usize];
                            rt.set_fullscreen(mode as u8);
                        }
                    }

                    if old.voxels != self.user_config.voxels {
                        if self.user_config.voxels {
                            if self.voxel_manager.is_none() {
                                self.voxel_manager = load_voxels(
                                    &self.options,
                                    &self.game.wad_data,
                                    self.game.game_type.mode,
                                    self.game.pic_data.pwad_sprite_overrides(),
                                );
                            }
                            if let Some(ref vm) = self.voxel_manager {
                                if let Some(rt) = self.render_backend.as_mut() {
                                    rt.set_voxel_manager(vm.clone());
                                }
                            }
                        } else if let Some(rt) = self.render_backend.as_mut() {
                            rt.clear_voxel_manager();
                        }
                    }

                    if old.music_type != self.user_config.music_type {
                        let type_val = self.game.config_values
                            [gamestate_traits::ConfigKey::MusicType as usize];
                        let _ = self
                            .game
                            .sound_cmd
                            .send(sound_common::SoundAction::SetMusicType(type_val));
                        self.game.replay_current_music();
                    }

                    if old.mouse_sensitivity != self.user_config.mouse_sensitivity {
                        self.input
                            .events
                            .set_mouse_scale((self.user_config.mouse_sensitivity, 1));
                    }
                    if old.invert_y != self.user_config.invert_y {
                        self.input.events.set_invert_y(self.user_config.invert_y);
                    }

                    if old.hud_size != self.user_config.hud_size {
                        let bar_h = if self.user_config.hud_size == 1 {
                            STBAR_HEIGHT
                        } else {
                            0
                        };
                        if let Some(rt) = self.render_backend.as_mut() {
                            rt.set_statusbar_height(bar_h);
                        }
                    }

                    if old.renderer != self.user_config.renderer
                        || old.hi_res != self.user_config.hi_res
                    {
                        set_lookdirs_hires(self.user_config.hi_res);
                        let prev_state = self.menu.as_ref().map(|m| m.save_state());
                        let old_rt = self.render_backend.take().unwrap();
                        let mut new_rt = old_rt.resize(
                            self.user_config.hi_res,
                            self.options.dev_parm,
                            &self.debug_draw,
                            self.user_config.renderer.into(),
                        );
                        if self.user_config.hud_size == 1 {
                            new_rt.set_statusbar_height(STBAR_HEIGHT);
                        }
                        if self.user_config.voxels {
                            if let Some(ref vm) = self.voxel_manager {
                                new_rt.set_voxel_manager(vm.clone());
                            }
                        }
                        self.render_backend = Some(new_rt);
                        let mut new_menu = GameMenu::new(
                            self.game.game_type.mode,
                            &self.game.wad_data,
                            self.render_backend.as_ref().unwrap().buffer_size().width(),
                        );
                        new_menu.init(&self.game);
                        if let Some(state) = prev_state {
                            new_menu.restore_state(state);
                        }
                        self.menu = Some(new_menu);
                    }
                }

                if let Some(fps) = self.timestep.frame_rate() {
                    if let Some(rt) = self.render_backend.as_mut() {
                        if self.user_config.show_fps {
                            rt.set_debug_line(format!("FPS {}", fps.frames));
                        } else {
                            rt.set_debug_line(String::new());
                        }
                    }
                    coarse_prof::write(&mut std::io::stdout()).unwrap();
                }

                if let Some(interval) = self.frame_interval {
                    self.next_frame += interval;
                    // Prevent spiral if we fell behind.
                    let now = Instant::now();
                    if self.next_frame < now {
                        self.next_frame = now;
                    }
                    event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
                } else if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion {
            delta,
        } = event
        {
            let xrel = self.input.events.apply_mouse_accel(delta.0 as f32) as i32;
            let yrel = self.input.events.apply_mouse_accel(delta.1 as f32) as i32;
            self.input.events.apply_mouse_sensitivity((xrel, yrel));
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.frame_interval.is_some() {
            // Vsync: WaitUntil handles wakeup timing. request_redraw was
            // already issued by the NewEvents/WaitCancelled path below.
            return;
        }
        // Uncapped: redraw as fast as possible.
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if self.frame_interval.is_some() {
            if matches!(
                cause,
                winit::event::StartCause::ResumeTimeReached { .. } | winit::event::StartCause::Init
            ) {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
}
