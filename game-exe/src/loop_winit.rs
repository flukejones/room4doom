//! winit `ApplicationHandler`-based game loop.

use std::sync::Arc;

use gameplay::log::{info, warn};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{GameRenderer, SubsystemTrait};
use hud_doom::Messages;
use input::InputState;
use intermission_doom::Intermission;
use menu_doom::MenuDoom;
use render_target::{DisplayBackend, RenderTarget};
use software3d::DebugDrawOptions;
use statusbar_doom::Statusbar;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::CLIOptions;
use crate::cheats::Cheats;
use crate::d_main::{d_display, input_responder, run_game_tic, set_lookdirs, update_sound};
use crate::timestep::TimeStep;

use finale_doom::Finale;

/// All game state owned by the winit event loop.
pub struct DoomApp {
    game: Game,
    input: InputState,
    cheats: Cheats,
    timestep: TimeStep,
    render_target: Option<RenderTarget>,
    menu: Option<MenuDoom>,
    machines: GameSubsystem<Intermission, Statusbar, Messages, Finale>,
    options: CLIOptions,
    debug_draw: DebugDrawOptions,
    window: Option<Arc<Window>>,
}

impl DoomApp {
    /// Create the app with game state ready. Window is created on `resumed`.
    pub fn new(game: Game, input: InputState, options: CLIOptions) -> Self {
        let debug_draw = options.debug_draw();
        let machines = GameSubsystem {
            statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
            intermission: Intermission::new(game.game_type.mode, &game.wad_data),
            hud_msgs: Messages::new(&game.wad_data),
            finale: Finale::new(&game.wad_data),
        };
        Self {
            game,
            input,
            cheats: Cheats::new(),
            timestep: TimeStep::new(true),
            render_target: None,
            menu: None,
            machines,
            options,
            debug_draw,
            window: None,
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
                        let mut by_res: std::collections::BTreeMap<(u32, u32), Vec<u32>> =
                            std::collections::BTreeMap::new();
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

        if matches!(self.options.vsync, Some(false)) {
            info!("vsync=off requested but softbuffer backend has no vsync control");
        }

        let display = DisplayBackend::new_softbuffer(window.clone());

        set_lookdirs(&self.options);
        let render_target = RenderTarget::new(
            self.options.hi_res.unwrap_or(true),
            self.options.dev_parm,
            &self.debug_draw,
            display,
            self.options.rendering.unwrap_or_default().into(),
        );
        let mut menu = MenuDoom::new(
            self.game.game_type.mode,
            &self.game.wad_data,
            render_target.buffer_size().width(),
        );
        menu.init(&self.game);

        if self.options.episode.is_none() && self.options.map.is_none() {
            self.game.start_title();
        }
        info!("Started title sequence");

        self.render_target = Some(render_target);
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
                    let display = DisplayBackend::new_softbuffer(window.clone());
                    set_lookdirs(&self.options);
                    let rt = RenderTarget::new(
                        self.options.hi_res.unwrap_or(true),
                        self.options.dev_parm,
                        &self.debug_draw,
                        display,
                        self.options.rendering.unwrap_or_default().into(),
                    );
                    let mut menu = MenuDoom::new(
                        self.game.game_type.mode,
                        &self.game.wad_data,
                        rt.buffer_size().width(),
                    );
                    menu.init(&self.game);
                    self.render_target = Some(rt);
                    self.menu = Some(menu);
                }
            }
            WindowEvent::RedrawRequested => {
                if !self.game.running() {
                    event_loop.exit();
                    return;
                }

                let rt = self
                    .render_target
                    .as_mut()
                    .expect("render target not initialized");
                let menu = self.menu.as_mut().expect("menu not initialized");

                self.timestep.run_this(|tics| {
                    run_game_tic(
                        &mut self.game,
                        &mut self.input,
                        menu,
                        &mut self.machines,
                        tics,
                    );
                });

                update_sound(&self.game);
                d_display(rt, menu, &mut self.machines, &mut self.game);

                if let Some(fps) = self.timestep.frame_rate() {
                    rt.set_debug_line(format!("FPS {}", fps.frames));
                    coarse_prof::write(&mut std::io::stdout()).unwrap();
                }

                if let Some(window) = &self.window {
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
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
