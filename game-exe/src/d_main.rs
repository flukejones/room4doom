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

use gameplay::MapObject;
use gameplay::log::error;
use gameplay::tic_cmd::{BASELOOKDIRMAX, BASELOOKDIRMIN, LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{DrawBuffer, GameRenderer, GameState, KeyCode, SubsystemTrait};
use hud_util::{draw_patch, fullscreen_scale};
use input::InputState;
use sound_common::SoundAction;
use wad::types::{BLACK, WadPatch};

use crate::CLIOptions;
use crate::cheats::Cheats;

pub(crate) const fn set_lookdirs(options: &CLIOptions) {
    unsafe {
        LOOKDIRMIN = BASELOOKDIRMIN;
        LOOKDIRMAX = BASELOOKDIRMAX;
        if matches!(options.hi_res, Some(true) | None) {
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
    tics: f32,
) {
    if game.demo.advance {
        game.do_advance_demo();
    }
    if !menu.ticker(game) && game.wipe_game_state != GameState::ForceWipe {
        game.ticker(machinations);
    }
    game.game_tic = tics as u32;

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
                x: mobj.xy.x,
                y: mobj.xy.y,
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
pub(crate) fn d_display<R>(
    render_target: &mut R,
    menu: &mut impl SubsystemTrait,
    machines: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    game: &mut Game,
) where
    R: GameRenderer,
{
    let wipe = game.gamestate != game.wipe_game_state;
    if wipe {
        // Capture old frame on first wipe frame
        render_target.start_wipe();
    }
    let automap_active = false;

    match game.gamestate {
        GameState::Level => {
            if !automap_active {
                match game.level {
                    Some(ref mut level) => {
                        if !game.players_in_game[game.consoleplayer] {
                            return;
                        }
                        if game.players[0].mobj().is_none() {
                            error!(
                                "Active console player has no MapObject, can't render player view"
                            );
                            dbg!(game.players[0].mobj());
                            dbg!(game.players[1].mobj());
                            dbg!(game.players[2].mobj());
                            dbg!(game.players[3].mobj());
                        } else {
                            let player = &game.players[game.consoleplayer];
                            render_target.render_player_view(player, level, &mut game.pic_data);
                        }
                    }
                    _ => {}
                }
            }
            machines.statusbar.draw(render_target.frame_buffer());
            machines.hud_msgs.draw(render_target.frame_buffer());
        }
        GameState::Intermission => machines.intermission.draw(render_target.frame_buffer()),
        GameState::Finale => machines.finale.draw(render_target.frame_buffer()),
        GameState::DemoScreen => {
            if game.page.cache.name != game.page.name {
                let lump = game
                    .wad_data
                    .get_lump(game.page.name)
                    .expect("TITLEPIC missing");
                game.page.cache = WadPatch::from_lump(lump);
            }
            page_drawer(game, render_target.frame_buffer());
        }
        _ => {}
    }

    if wipe {
        // Overdraw old-frame columns on top of the new scene
        if render_target.do_wipe() {
            game.wipe_game_state = game.gamestate;
        } else {
        }
        menu.draw(render_target.frame_buffer());
    } else {
        menu.draw(render_target.frame_buffer());
    }
    render_target.flip_and_present();
}
