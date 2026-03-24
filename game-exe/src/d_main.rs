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

use game_config::tic_cmd::{BASELOOKDIRMAX, BASELOOKDIRMIN, LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gameplay::{MapObjFlag, MapObject, Player};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{GameState, KeyCode, SubsystemTrait};
use hud_util::{draw_patch, fullscreen_scale};
use input::InputState;
use level::LevelData;
use log::error;
use math::{Angle, Bam, FixedT};
use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;
use render_common::{DrawBuffer, GameRenderer, RenderPspDef, RenderView, draw_health_vignette};
use sound_common::SoundAction;
use wad::types::{BLACK, WadPalette, WadPatch};

use crate::CLIOptions;
use crate::cheats::Cheats;

/// Build a render view from the current player state.
/// Returns `None` when the player has no map object (e.g. during intermission).
fn build_render_view(
    player: &Player,
    level_data: &LevelData,
    frac: f32,
    game_tic: u32,
) -> Option<RenderView> {
    let mobj = player.mobj()?;
    let subsector_id = level_data
        .subsectors()
        .iter()
        .position(|ss| std::ptr::eq(ss, &*mobj.subsector))?;
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
        subsector_id,
        psprites,
        sector_lightlevel: mobj.subsector.sector.lightlevel as usize,
        player_mobj_id: mobj as *const _ as usize,
        frac,
        frac_fp,
        game_tic,
    })
}

/// Load voxel models from the CLI-specified directory (if any) and attach
/// them to the render target.
pub(crate) fn load_voxels(
    options: &CLIOptions,
    wad: &wad::WadData,
    game_mode: game_config::GameMode,
    pwad_overrides: &std::collections::HashSet<String>,
) -> Option<std::sync::Arc<pic_data::VoxelManager>> {
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
        log::warn!("Voxel path is not a directory or PK3: {}", voxel_path);
        return None;
    };
    println!("]");
    if mgr.is_empty() {
        log::warn!("No voxel models loaded from {}", voxel_path);
        return None;
    }
    Some(Arc::new(mgr))
}

pub(crate) const fn set_lookdirs(options: &CLIOptions) {
    set_lookdirs_hires(matches!(options.hi_res, Some(true) | None));
}

pub(crate) const fn set_lookdirs_hires(hi_res: bool) {
    unsafe {
        LOOKDIRMIN = BASELOOKDIRMIN;
        LOOKDIRMAX = BASELOOKDIRMAX;
        if hi_res {
            LOOKDIRMAX *= 2;
            LOOKDIRMIN *= 2;
        }
        LOOKDIRS = 1 + LOOKDIRMIN + LOOKDIRMAX;
    }
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
        let uid = mobj as *const MapObject as usize;
        game.sound_cmd
            .send(SoundAction::UpdateListener {
                uid,
                x: mobj.x.to_f32(),
                y: mobj.y.to_f32(),
                angle: mobj.angle.rad(),
            })
            .unwrap();
    }
}

fn page_drawer(game: &mut Game, draw_buf: &mut impl DrawBuffer) {
    draw_buf.buf_mut().fill(BLACK);
    let (sx, sy) = fullscreen_scale(draw_buf);
    let x = (draw_buf.size().width_f32() - 320.0 * sx) / 2.0;
    let palette = game.pic_data.wad_palette();
    draw_patch(&game.page.cache, x, 0.0, sx, sy, palette, draw_buf);
}

/// D_Display — draw the current frame.
pub(crate) fn d_display<R: GameRenderer>(
    render_backend: &mut R,
    menu: &mut impl SubsystemTrait,
    machines: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    game: &mut Game,
    frac: f32,
) {
    let wipe = game.gamestate != game.wipe_game_state;
    if wipe {
        // Capture old frame on first wipe frame
        render_backend.start_wipe();
    }
    // Disable interpolation during wipe — the old frame covers the transition
    // and prev state may not be valid for the new level.
    let frac = if render_backend.is_wiping() || game.frozen {
        1.0
    } else {
        frac
    };
    let automap_active = false;

    match game.gamestate {
        GameState::Level => {
            if !automap_active {
                match game.level {
                    Some(ref mut level) => {
                        if !game.players_in_game[game.consoleplayer] {
                            return;
                        }
                        // Interpolate sector heights and light levels for smooth rendering
                        level.level_data.apply_render_interpolation(frac);

                        let player = &game.players[game.consoleplayer];
                        if let Some(view) =
                            build_render_view(player, &level.level_data, frac, game.game_tic)
                        {
                            game.pic_data.set_player_palette(
                                player.status.damagecount,
                                player.status.bonuscount,
                                player.status.powers[gameplay::PowerType::Strength as usize],
                                player.status.powers[gameplay::PowerType::IronFeet as usize],
                            );
                            render_backend.render_player_view(
                                &view,
                                &level.level_data,
                                &mut game.pic_data,
                            );
                            if game.config_values
                                [gamestate_traits::ConfigKey::HealthVignette as usize]
                                != 0
                            {
                                let fb = render_backend.frame_buffer();
                                let w = fb.size().width_usize();
                                let vh = fb.size().view_height_usize();
                                let pitch = fb.pitch();
                                draw_health_vignette(
                                    fb.buf_mut(),
                                    pitch,
                                    w,
                                    vh,
                                    player.status.health,
                                );
                            }
                        } else {
                            error!(
                                "Active console player has no MapObject, can't render player view"
                            );
                        }

                        // Restore true post-tic sector values
                        level.level_data.restore_render_interpolation();
                    }
                    _ => {}
                }
            }
            machines.statusbar.draw(render_backend.frame_buffer());
            machines.hud_msgs.draw(render_backend.frame_buffer());
        }
        GameState::Intermission => machines.intermission.draw(render_backend.frame_buffer()),
        GameState::Finale => machines.finale.draw(render_backend.frame_buffer()),
        GameState::DemoScreen => {
            if game.page.cache.name != game.page.name {
                let lump = game
                    .wad_data
                    .get_lump(game.page.name)
                    .expect("TITLEPIC missing");
                game.page.cache = WadPatch::from_lump(lump);
            }
            page_drawer(game, render_backend.frame_buffer());
        }
        _ => {}
    }

    if wipe {
        // Overdraw old-frame columns on top of the new scene
        if render_backend.do_wipe() {
            game.wipe_game_state = game.gamestate;
            // Snap player prev_render so first interpolated frame after wipe
            // doesn't jump from a stale position
            let player = &mut game.players[game.consoleplayer];
            player.save_prev_render();
        }
        menu.draw(render_backend.frame_buffer());
    } else {
        menu.draw(render_backend.frame_buffer());
    }
    render_backend.flip_and_present();
}
