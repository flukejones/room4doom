//! The one entry point for touching a special line.
//!
//! `P_CrossSpecialLine`, `P_UseSpecialLine` and `P_ShootSpecialLine` are the
//! same dispatch under three [`Activation`]s: gate the thing, check the
//! decoded trigger fires for that activation, run the action, then apply the
//! consume/switch-texture rules.

use log::{debug, warn};
use sound_common::SfxName;

use crate::doom_def::Card;
use crate::env::effects::light_presets::{
    ev_start_light_strobing, ev_turn_light_on, ev_turn_tag_lights_off,
};
use crate::env::effects::mover_presets::{
    ev_build_stairs, ev_do_ceiling, ev_do_donut, ev_do_door, ev_do_floor, ev_do_platform,
    ev_stop_platform, ev_vertical_door,
};
use crate::env::switch::change_switch_texture;
use crate::env::teleport::teleport;
use crate::info::MapObjKind;
use crate::lang::english::{
    PD_ALL3, PD_ALL6, PD_ANY, PD_BLUEC, PD_BLUEK, PD_BLUEO, PD_BLUES, PD_REDC, PD_REDK, PD_REDO,
    PD_REDS, PD_YELLOWC, PD_YELLOWK, PD_YELLOWO, PD_YELLOWS,
};
use crate::level::LevelState;
use crate::thing::MapObject;
use level::MapPtr;
use level::flags::LineDefFlags;
use level::map_defs::LineDef;
use level::special_encode::{
    Activation, CeilKind, DoorLock, FloorKind, LineAction, LineSpecial, LockKind, MoverKind,
    MoverSpec, Trigger,
};

/// Touch a line: cross it, use it, or shoot it. Returns whether the line was
/// acted on — the "used" answer a monster's `P_Move` consumes.
pub fn activate_line(
    mut line: MapPtr<LineDef>,
    thing: &mut MapObject,
    side: usize,
    activation: Activation,
) -> bool {
    let Some(special) = line.special else {
        return false;
    };
    let LineSpecial::Triggered {
        trigger,
        action,
    } = special
    else {
        // A standing effect or an unhandled number: still a special line, so a
        // use lands on it rather than passing through to what is behind.
        if let LineSpecial::Unimplemented(n) = special {
            warn!("Unimplemented line special {n}");
        }
        return activation == Activation::Use;
    };
    if trigger.activation() != activation {
        return false;
    }
    if !can_touch(thing, &line, action, activation) {
        return false;
    }

    assert!(!thing.level.is_null(), "Thing had a bad level pointer");
    let level: &mut LevelState = unsafe { &mut *thing.level };
    let acted = run(action, trigger, line.clone(), thing, side, level);
    if !acted {
        return false;
    }

    match activation {
        // A walk trigger clears itself; a switch or gun trigger is cleared by
        // the texture flip, which manual doors never get.
        Activation::Cross => {
            if !trigger.is_repeatable() && action.consumed_when_once() {
                line.special = None;
            }
        }
        Activation::Use | Activation::Shoot => {
            if !matches!(action, LineAction::Mover(m) if m.manual) {
                change_switch_texture(
                    line,
                    trigger.is_repeatable(),
                    &level.switch_list,
                    &mut level.button_list,
                    &level.snd_command,
                    &mut level.level_data.bsp_3d,
                );
            }
        }
    }
    true
}

/// The pre-dispatch gates OG applies to the thing rather than the special.
fn can_touch(
    thing: &MapObject,
    line: &LineDef,
    action: LineAction,
    activation: Activation,
) -> bool {
    let monster_teleport = action
        == LineAction::Teleport {
            monsters_only: true,
        };
    if monster_teleport && thing.player().is_some() {
        return false;
    }
    if thing.player().is_some() {
        return true;
    }
    let reaches = match activation {
        // Projectiles cross lines without firing anything.
        Activation::Cross => !matches!(
            thing.kind,
            MapObjKind::MT_ROCKET
                | MapObjKind::MT_PLASMA
                | MapObjKind::MT_BFG
                | MapObjKind::MT_TROOPSHOT
                | MapObjKind::MT_HEADSHOT
                | MapObjKind::MT_BRUISERSHOT
        ),
        // A monster cannot work out a secret door.
        Activation::Use => !line.flags.contains(LineDefFlags::Secret),
        Activation::Shoot => true,
    };
    reaches && action.allows_monster()
}

/// Run the action, returning whether it did anything.
fn run(
    action: LineAction,
    trigger: Trigger,
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    side: usize,
    level: &mut LevelState,
) -> bool {
    match action {
        LineAction::Mover(spec) => move_sectors(spec, trigger, line, thing, level),
        LineAction::Exit {
            secret: false,
        } => {
            level.do_exit_level();
            true
        }
        LineAction::Exit {
            secret: true,
        } => {
            level.do_secret_exit_level();
            true
        }
        LineAction::Teleport {
            ..
        } => {
            teleport(line, side, thing, level);
            true
        }
        LineAction::LightTurnOn(level_to) => {
            debug!("line-special: turn light to {level_to}");
            ev_turn_light_on(line, usize::from(level_to), level);
            true
        }
        LineAction::LightsOffTag => {
            ev_turn_tag_lights_off(line, level);
            true
        }
        LineAction::StartStrobing => {
            ev_start_light_strobing(line, level);
            true
        }
        LineAction::StopPlat => {
            ev_stop_platform(line, level);
            true
        }
    }
}

/// Build the mover(s) a special names.
fn move_sectors(
    spec: MoverSpec,
    trigger: Trigger,
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    if spec.composite {
        let ceil = ev_do_ceiling(line.clone(), CeilKind::RaiseToHighest, level);
        let floor = ev_do_floor(line, FloorKind::LowerFloorToLowest, level);
        return ceil || floor;
    }
    match spec.kind {
        MoverKind::Floor(FloorKind::DonutRaise) => ev_do_donut(line, level),
        MoverKind::Floor(kind) => ev_do_floor(line, kind, level),
        MoverKind::Ceiling(kind) => ev_do_ceiling(line, kind, level),
        MoverKind::Door(kind) if spec.manual => {
            // Vanilla EV_VerticalDoor: a failed key check still counts as
            // "used", so a monster bumping a locked door keeps its move.
            let unlocked = match spec.lock {
                Some(lock) => can_unlock(thing, lock, true),
                None => true,
            };
            if unlocked {
                ev_vertical_door(line, thing, kind, trigger.is_repeatable(), level);
            }
            true
        }
        MoverKind::Door(kind) => {
            // EV_DoLockedDoor: no key means no door and no switch texture.
            let unlocked = match spec.lock {
                Some(lock) => can_unlock(thing, lock, false),
                None => true,
            };
            unlocked && ev_do_door(line, kind, level)
        }
        MoverKind::Lift(kind) => ev_do_platform(line, kind, spec.amount, level),
        MoverKind::Stairs(kind) => ev_build_stairs(line, kind, level),
    }
}

/// BOOM `P_CanUnlockGenDoor`: does the player hold the lock's key? On failure
/// sets the "you need a ... key" message and plays the oof.
fn can_unlock(thing: &mut MapObject, lock: DoorLock, manual: bool) -> bool {
    let Some(player) = thing.player_mut() else {
        return false;
    };
    let has = |c: Card| player.status.cards[c as usize];
    let sac = lock.skulls_are_cards;
    // Card-or-skull locks (all vanilla ones) keep the vanilla messages, split
    // by door vs switch; BOOM-authored exact locks get card/skull-specific ones.
    let vanilla = |k_msg, o_msg| if manual { k_msg } else { o_msg };
    let (ok, message) = match lock.kind {
        LockKind::AnyKey => {
            let any = has(Card::Bluecard)
                || has(Card::Blueskull)
                || has(Card::Redcard)
                || has(Card::Redskull)
                || has(Card::Yellowcard)
                || has(Card::Yellowskull);
            (any, PD_ANY)
        }
        LockKind::BlueCard => (
            has(Card::Bluecard) || (sac && has(Card::Blueskull)),
            if sac {
                vanilla(PD_BLUEK, PD_BLUEO)
            } else {
                PD_BLUEC
            },
        ),
        LockKind::RedCard => (
            has(Card::Redcard) || (sac && has(Card::Redskull)),
            if sac {
                vanilla(PD_REDK, PD_REDO)
            } else {
                PD_REDC
            },
        ),
        LockKind::YellowCard => (
            has(Card::Yellowcard) || (sac && has(Card::Yellowskull)),
            if sac {
                vanilla(PD_YELLOWK, PD_YELLOWO)
            } else {
                PD_YELLOWC
            },
        ),
        LockKind::BlueSkull => (
            has(Card::Blueskull) || (sac && has(Card::Bluecard)),
            if sac {
                vanilla(PD_BLUEK, PD_BLUEO)
            } else {
                PD_BLUES
            },
        ),
        LockKind::RedSkull => (
            has(Card::Redskull) || (sac && has(Card::Redcard)),
            if sac {
                vanilla(PD_REDK, PD_REDO)
            } else {
                PD_REDS
            },
        ),
        LockKind::YellowSkull => (
            has(Card::Yellowskull) || (sac && has(Card::Yellowcard)),
            if sac {
                vanilla(PD_YELLOWK, PD_YELLOWO)
            } else {
                PD_YELLOWS
            },
        ),
        LockKind::AllKeys => {
            let all = if sac {
                (has(Card::Bluecard) || has(Card::Blueskull))
                    && (has(Card::Redcard) || has(Card::Redskull))
                    && (has(Card::Yellowcard) || has(Card::Yellowskull))
            } else {
                has(Card::Bluecard)
                    && has(Card::Blueskull)
                    && has(Card::Redcard)
                    && has(Card::Redskull)
                    && has(Card::Yellowcard)
                    && has(Card::Yellowskull)
            };
            (all, if sac { PD_ALL3 } else { PD_ALL6 })
        }
    };
    if !ok {
        player.message = Some(message);
        player.start_sound(SfxName::Oof);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestLevel, rng_guard};
    use level::special_encode::decode_line;

    /// Locked doors refuse without the key and open with it, in both the
    /// manual (26, PD_BLUEK) and key switch (137, PD_YELLOWO with a skull
    /// accepted for a card) forms — the lock bits through the real
    /// decode + dispatch path. The demo maps barely exercise locks.
    #[test]
    fn locked_doors_enforce_keys() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        let player = level.spawn_player(1056, -3616);

        // Any manual door line will do (found by its authored special);
        // relock it blue.
        let ld_idx = level
            .level_data()
            .linedefs
            .iter()
            .position(|l| l.default_special == 1)
            .expect("E1M1 has a manual door");
        let mut ld = MapPtr::new(&mut level.level_data_mut().linedefs[ld_idx]);
        ld.special = decode_line(26);

        let thing = unsafe { &mut *player };
        let used = activate_line(ld.clone(), thing, 0, Activation::Use);
        assert!(used, "vanilla counts a failed key check as used");
        assert!(ld.backsector.as_ref().unwrap().specialdata.is_none());
        assert_eq!(level.players()[0].message, Some(PD_BLUEK));

        level.parts_mut().1[0].status.cards[Card::Bluecard as usize] = true;
        let thing = unsafe { &mut *player };
        activate_line(ld.clone(), thing, 0, Activation::Use);
        assert!(
            ld.backsector.as_ref().unwrap().specialdata.is_some(),
            "manual door must open with the blue card"
        );

        // Switch form on ld195 (tag 2): yellow once-only blaze open.
        let mut sw = MapPtr::new(&mut level.level_data_mut().linedefs[195]);
        sw.special = decode_line(137);
        let thing = unsafe { &mut *player };
        activate_line(sw.clone(), thing, 0, Activation::Use);
        let tagged = |level: &TestLevel| {
            level
                .level_data()
                .sectors
                .iter()
                .filter(|s| s.tag == 2)
                .map(|s| s.specialdata.is_some())
                .collect::<Vec<_>>()
        };
        assert!(
            tagged(&level).iter().all(|d| !d),
            "no key: tagged sectors must stay idle"
        );
        assert_eq!(level.players()[0].message, Some(PD_YELLOWO));

        // A skull satisfies a card lock on vanilla key doors.
        level.parts_mut().1[0].status.cards[Card::Yellowskull as usize] = true;
        let thing = unsafe { &mut *player };
        activate_line(sw.clone(), thing, 0, Activation::Use);
        assert!(
            tagged(&level).iter().all(|d| *d),
            "yellow skull must open the yellow card switch-door"
        );
    }

    /// A line only fires for the way of touching it that its trigger names.
    /// Walking into a switch, or pressing a walk-over line, must do nothing —
    /// vanilla reaches each number from one dispatch table only.
    #[test]
    fn wrong_activation_does_nothing() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        let player = level.spawn_player(1056, -3616);
        let idle = |level: &TestLevel| {
            level
                .level_data()
                .sectors
                .iter()
                .filter(|s| s.tag == 2)
                .all(|s| s.specialdata.is_none())
        };

        // ld195 drives tag 2. As a walk-once lift, a use must not move it.
        let mut ld = MapPtr::new(&mut level.level_data_mut().linedefs[195]);
        ld.special = decode_line(10);
        let thing = unsafe { &mut *player };
        assert!(!activate_line(ld.clone(), thing, 0, Activation::Use));
        assert!(idle(&level), "walk lift must not answer a use");

        // As a switch-once door, crossing it must not open it.
        ld.special = decode_line(103);
        let thing = unsafe { &mut *player };
        assert!(!activate_line(ld.clone(), thing, 0, Activation::Cross));
        assert!(idle(&level), "switch door must not answer a cross");

        // The matching activation does fire, so the line itself is sound.
        let thing = unsafe { &mut *player };
        assert!(activate_line(ld.clone(), thing, 0, Activation::Use));
        assert!(!idle(&level), "switch door must open on a use");
    }
}
