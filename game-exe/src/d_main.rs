//! `d_main()` is the main loop. The task it has is to do a number of things:
//! 1. Set up the `Game`
//! 2. Run the ticks for all subsystems (input, menus, hud etc)
//! 3. Display the results on screen
//!
//! Most things are separated out as much as possible. The `Game` is its
//! own world, and various subsystems rely on getting data from it. These
//! subsystems are:
//! - `Statusbar`
//! - `Intermission`
//! - `Messages`
//! - `Finale`
//! - others may be added such as automap
//!
//! Note that the sound system runs on its own thread.
//!
//! `Game` also contains a lot of state ranging from the world state, demo
//! playback state, options used to setup the world, players and their stats,
//! and the overall gamestate.

use std::ptr;

use gameplay::{MapObjFlag, Player};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{GameState, KeyCode, SubsystemTrait};
use hud_util::{draw_patch, draw_text_line, fullscreen_scale, hud_scale, measure_text_line};
use input::InputState;
use log::error;
use math::{Angle, Bam, FixedT};
#[cfg(feature = "wgpu3d")]
use pic_data::resolve_tint_state;
use render_backend::RenderStack;
#[cfg(feature = "wgpu3d")]
use render_backend::ScreenEffects;
use render_common::{ByteOrder, DrawBuffer, PixelFmt, RenderPspDef, RenderView};
use sound_common::SoundAction;
use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;
use wad::types::{BLACK, WadPalette, WadPatch};

use crate::CLIOptions;
use crate::cheats::Cheats;

/// Build a render view from the current player state.
/// Returns `None` when the player has no map object (e.g. during intermission).
fn build_render_view(player: &Player, frac: f32, game_tic: u32) -> Option<RenderView> {
    let mobj = player.mobj()?;
    let prev = &player.prev_render;

    // f32 boundary: frac originates from Timestep (Instant timing division)
    let frac_fp = FixedT::from_f32(frac);
    let lerp_fp = |a: FixedT, b: FixedT| -> FixedT { a + (b - a) * frac_fp };

    let curr_bam = mobj.angle.to_bam();
    let angle_delta = curr_bam.wrapping_sub(prev.angle_bam) as i32;
    let interp_bam = prev
        .angle_bam
        .wrapping_add((angle_delta as f32 * frac) as i32 as u32);

    let lookdir_delta = (player.lookdir - prev.lookdir) as f32;
    let interp_lookdir = prev.lookdir as f32 + lookdir_delta * frac;

    let psprites = std::array::from_fn(|i| {
        let psp = &player.psprites[i];
        match psp.state {
            Some(s) => RenderPspDef {
                active: true,
                sprite: s.sprite as usize,
                frame: s.frame,
                sx: prev.psp_sx[i] + (psp.sx - prev.psp_sx[i]) * frac,
                sy: prev.psp_sy[i] + (psp.sy - prev.psp_sy[i]) * frac,
            },
            None => RenderPspDef::default(),
        }
    });

    Some(RenderView {
        x: lerp_fp(prev.x, mobj.x),
        y: lerp_fp(prev.y, mobj.y),
        z: lerp_fp(prev.z, mobj.z),
        viewz: lerp_fp(prev.viewz, player.viewz),
        viewheight: player.viewheight,
        angle: Angle::<Bam>::from_bam(interp_bam),
        lookdir: interp_lookdir * PI / i32::MAX as f32,
        fixedcolormap: player.fixedcolormap as usize,
        extralight: player.extralight,
        is_shadow: mobj.flags.contains(MapObjFlag::Shadow),
        psprites,
        sector_lightlevel: mobj.subsector.sector.lightlevel,
        player_mobj_id: ptr::from_ref(mobj) as usize,
        frac,
        frac_fp,
        game_tic,
    })
}

/// Build the GPU `ScreenEffects` (player tint + invuln + health bleed) from the
/// console player's status. `bleed_enabled` is the user config toggle.
#[cfg(feature = "wgpu3d")]
fn build_screen_effects(player: &Player, bleed_enabled: bool) -> ScreenEffects {
    let (damagecount, radsuit) = resolve_tint_state(
        player.status.damagecount,
        player.status.powers[gameplay::PowerType::Strength as usize],
        player.status.powers[gameplay::PowerType::IronFeet as usize],
    );
    ScreenEffects {
        damagecount,
        bonuscount: player.status.bonuscount,
        radsuit,
        fixedcolormap: player.fixedcolormap as usize,
        health: player.status.health,
        bleed_enabled,
    }
}

/// Load voxel models from the CLI-specified directory (if any) and attach
/// them to the render target.
pub(crate) fn load_voxels(
    options: &CLIOptions,
    wad: &wad::WadData,
    game_mode: game_config::GameMode,
    pwad_overrides: &std::collections::HashSet<String>,
) -> Option<Arc<pic_data::VoxelManager>> {
    let voxel_path = options.voxels.as_ref()?;
    let path = Path::new(voxel_path);
    let doom_palette: Vec<u8> = wad
        .lump_iter::<WadPalette>("PLAYPAL")
        .next()
        .map(|pal| {
            let mut rgb = Vec::with_capacity(768);
            for c in &pal.0 {
                rgb.push((*c >> 16) as u8);
                rgb.push((*c >> 8) as u8);
                rgb.push(*c as u8);
            }
            rgb
        })
        .unwrap_or_default();
    print!("Init voxel data  [");
    let is_pk3 = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pk3"));
    let mgr = if is_pk3 {
        pic_data::VoxelManager::load_from_pk3(
            path,
            game_mode,
            &gameplay::SPRNAMES,
            &doom_palette,
            pwad_overrides,
        )
    } else if path.is_dir() {
        pic_data::VoxelManager::load_from_directory(
            path,
            &gameplay::SPRNAMES,
            &doom_palette,
            pwad_overrides,
        )
    } else {
        log::warn!("Voxel path is not a directory or PK3: {voxel_path}");
        return None;
    };
    println!("]");
    if mgr.is_empty() {
        log::warn!("No voxel models loaded from {voxel_path}");
        return None;
    }
    Some(Arc::new(mgr))
}

/// Handle key-down for menu/cheat consumption. Returns true if consumed.
pub(crate) fn input_responder(
    sc: KeyCode,
    game: &mut Game,
    menu: &mut impl SubsystemTrait,
    machinations: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    cheats: &mut Cheats,
) -> bool {
    if game.level.is_some() {
        cheats.check_input(sc, game);
    }

    if menu.responder(sc, game) {
        return true;
    }

    if machinations.hud_msgs.responder(sc, game) {
        return true;
    }

    if game.level.is_none() {
        #[allow(clippy::collapsible_match)] // can't do this with &mut self methods
        match game.gamestate {
            GameState::Intermission => {
                if machinations.intermission.responder(sc, game) {
                    return true;
                }
            }
            GameState::Finale => {
                if machinations.finale.responder(sc, game) {
                    return true;
                }
            }
            _ => {}
        }
    }

    false
}

/// Advance game state by one tic: demo advance, menu tick, game tick,
/// and build the tic command from current input.
pub(crate) fn run_game_tic(
    game: &mut Game,
    input: &mut InputState,
    menu: &mut impl SubsystemTrait,
    machinations: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
) {
    if game.demo.advance {
        game.do_advance_demo();
    }
    let menu_active = menu.ticker(game);
    let in_user_game = game.gamestate == GameState::Level && !game.demo.playback;
    let menu_blocks = menu_active && in_user_game;
    if !menu_blocks && game.wipe_game_state != GameState::ForceWipe {
        game.ticker(machinations);
        game.frozen = false;
    } else {
        // Tick subsystems for config reads even when gameplay is frozen
        machinations.statusbar.ticker(game);
        machinations.hud_msgs.ticker(game);
        game.frozen = true;
    }
    game.game_tic += 1;

    let console_player = game.consoleplayer;
    if game.gamestate == game.wipe_game_state {
        let cmd = input.events.build_tic_cmd(&input.config);
        game.netcmds[console_player][0] = cmd;
    }
}

/// Update the sound listener position from the console player.
pub(crate) fn update_sound(game: &Game) {
    if let Some(mobj) = game.players[game.consoleplayer].mobj() {
        let uid = ptr::from_ref(mobj) as usize;
        if let Err(e) = game.sound_cmd.send(SoundAction::UpdateListener {
            uid,
            x: mobj.x.to_f32(),
            y: mobj.y.to_f32(),
            angle: mobj.angle.rad(),
        }) {
            error!("Could not send listener update, sound thread gone: {e}");
        }
    }
}

fn page_drawer(game: &Game, draw_buf: &mut impl DrawBuffer) {
    let black = PixelFmt::from_argb(BLACK, ByteOrder::Argb);
    draw_buf.buf_mut().fill(black);
    let (sx, sy) = fullscreen_scale(draw_buf);
    let x = (draw_buf.size().width_f32() - 320.0 * sx) / 2.0;
    let palette = game.pic_data.wad_palette();
    draw_patch(&game.page.cache, x, 0.0, sx, sy, palette, draw_buf);
}

/// D_Display — drive one frame's rendering, and only that. One entry for every
/// renderer: the CPU software renderers write final pixels into the shared frame
/// and melt the previous frame for screen wipes; the GPU renderer records the
/// scene into a texture, draws UI into the same shared frame, and composites +
/// melts in shaders. The render kind is hidden behind [`RenderStack`]'s uniform
/// API. `fps` is drawn top-right when non-empty.
pub(crate) fn d_display<P: PixelFmt>(
    screen: &mut RenderStack<P>,
    menu: &mut impl SubsystemTrait,
    machines: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    game: &mut Game,
    frac: f32,
    fps: &str,
) {
    let wipe = game.gamestate != game.wipe_game_state;
    if wipe {
        // Fresh bleed pattern for the new game state.
        screen.reset_health_bleed();
        if !screen.is_wiping() {
            screen.start_wipe();
        }
    }
    // Disable interpolation during a wipe — the old frame covers the transition
    // and prev state may not be valid for the new level.
    let frac = if screen.is_wiping() || game.frozen {
        1.0
    } else {
        frac
    };

    match game.gamestate {
        GameState::Level => {
            if let Some(level) = game.level.as_mut()
                && game.players_in_game[game.consoleplayer]
            {
                level.level_data.apply_render_interpolation(frac);
                let player = &game.players[game.consoleplayer];
                if let Some(view) = build_render_view(player, frac, game.game_tic) {
                    game.pic_data.set_player_palette(
                        player.status.damagecount,
                        player.status.bonuscount,
                        player.status.powers[gameplay::PowerType::Strength as usize],
                        player.status.powers[gameplay::PowerType::IronFeet as usize],
                    );
                    #[cfg(feature = "wgpu3d")]
                    {
                        let bleed_enabled = game.config_values
                            [gamestate_traits::ConfigKey::HealthBleed as usize]
                            != 0;
                        let player = &game.players[game.consoleplayer];
                        screen.set_screen_effects(build_screen_effects(player, bleed_enabled));
                    }
                    screen.render_player_view(&view, &level.level_data, &mut game.pic_data);
                } else {
                    error!("Active console player has no MapObject, can't render player view");
                }
                level.level_data.restore_render_interpolation();
                // GPU-only: the textures now match the rendered frame; clear the
                // dirty bits so static frames skip re-upload. Idempotent; the next
                // interpolation re-dirties moving sectors, switches/scrollers
                // re-dirty texture state on the next tic.
                #[cfg(feature = "wgpu3d")]
                if screen.is_hardware_renderer() {
                    level.level_data.bsp_3d_mut().clear_geometry_dirty();
                    level.level_data.bsp_3d_mut().clear_texture_dirty();
                }
            }
            machines.statusbar.draw(&mut screen.ui_frame());
            machines.hud_msgs.draw(&mut screen.ui_frame());
        }
        GameState::Intermission => machines.intermission.draw(&mut screen.ui_frame()),
        GameState::Finale => machines.finale.draw(&mut screen.ui_frame()),
        GameState::DemoScreen => {
            refresh_title_page(game);
            page_drawer(game, &mut screen.ui_frame());
        }
        GameState::ForceWipe => {}
    }

    // Advance the wipe (CPU melts the previous frame; GPU steps the melt offsets).
    // The melt runs over several frames; advance the wipe state only once it has
    // fully completed. `build_tic_cmd` resumes when gamestate == wipe_game_state.
    if wipe && screen.do_wipe() {
        game.wipe_game_state = game.gamestate;
        // Snap player prev_render so the first interpolated frame after the wipe
        // doesn't jump from a stale position.
        let player = &mut game.players[game.consoleplayer];
        player.save_prev_render();
    }
    if !fps.is_empty() {
        let palette = game.pic_data.wad_palette();
        let ui = screen.ui_frame();
        let (sx, sy) = hud_scale(ui);
        let x = ui.size().width_f32() - measure_text_line(fps, sx) - 4.0 * sx;
        draw_text_line(fps, x, 2.0, sx, sy, palette, ui);
    }
    menu.draw(&mut screen.ui_frame());
    screen.present(wipe);
}

/// Refresh the cached TITLEPIC patch when the page name changed.
fn refresh_title_page(game: &mut Game) {
    if game.page.cache.name != game.page.name {
        let lump = game
            .wad_data
            .get_lump(game.page.name)
            .expect("TITLEPIC missing");
        game.page.cache = WadPatch::from_lump(lump);
    }
}
