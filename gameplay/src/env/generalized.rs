//! BOOM generalized + engine-extended linedef dispatch.
//!
//! All mover specials are normalised to generalized form at load (see
//! [`level::special_encode`]); these entry points decode them to the exact
//! room4doom mover kind and build the [`crate::env::effects::MoveEffect`]
//! preset for it.

use sound_common::SfxName;

use crate::doom_def::Card;
use crate::env::effects::mover_presets::{
    ev_build_stairs, ev_do_ceiling, ev_do_donut, ev_do_door, ev_do_floor, ev_do_platform,
    ev_vertical_door,
};
use crate::env::switch::change_switch_texture;
use crate::lang::english::{
    PD_ALL3, PD_ALL6, PD_ANY, PD_BLUEC, PD_BLUEK, PD_BLUEO, PD_BLUES, PD_REDC, PD_REDK, PD_REDO,
    PD_REDS, PD_YELLOWC, PD_YELLOWK, PD_YELLOWO, PD_YELLOWS,
};
use crate::level::LevelState;
use crate::thing::MapObject;
use level::MapPtr;
use level::map_defs::LineDef;
use level::special_encode::{
    self, CeilKind, DecodedSpecial, DoorLock, FloorKind, LockKind, MoverKind, Trigger,
};

pub fn is_generalized(special: u32) -> bool {
    special_encode::is_generalized(special)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TriggerClass {
    Walk,
    Switch,
    Gun,
    Push,
}

fn trigger_class(t: Trigger) -> TriggerClass {
    match t {
        Trigger::WalkOnce | Trigger::WalkMany => TriggerClass::Walk,
        Trigger::SwitchOnce | Trigger::SwitchMany => TriggerClass::Switch,
        Trigger::GunOnce | Trigger::GunMany => TriggerClass::Gun,
        Trigger::PushOnce | Trigger::PushMany => TriggerClass::Push,
    }
}

/// Entry point for walk-triggered generalized linedefs
pub fn handle_generalized_cross(
    mut line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
    is_monster: bool,
) -> bool {
    let Some(spec) = special_encode::decode(line.special) else {
        return false;
    };
    if trigger_class(spec.trigger) != TriggerClass::Walk {
        return false;
    }
    if is_monster && !spec.monster {
        return false;
    }

    let result = dispatch(spec, line.clone(), thing, level);

    if result && !spec.trigger.is_repeatable() {
        line.special = 0;
    }

    result
}

/// Entry point for switch/push-triggered generalized linedefs
pub fn handle_generalized_use(
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    let Some(spec) = special_encode::decode(line.special) else {
        return false;
    };
    let class = trigger_class(spec.trigger);
    if class != TriggerClass::Switch && class != TriggerClass::Push {
        return false;
    }
    if thing.player().is_none() && !spec.monster {
        return false;
    }

    let result = dispatch(spec, line.clone(), thing, level);

    // Manual doors are not switches: vanilla never changes their texture.
    if result && !spec.manual {
        change_switch_texture(
            line,
            spec.trigger.is_repeatable(),
            &level.switch_list,
            &mut level.button_list,
            &level.snd_command,
            &mut level.level_data.bsp_3d,
        );
    }

    result
}

/// Entry point for gun-triggered generalized linedefs
pub fn handle_generalized_shoot(
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    let Some(spec) = special_encode::decode(line.special) else {
        return false;
    };
    if trigger_class(spec.trigger) != TriggerClass::Gun {
        return false;
    }
    if thing.player().is_none() && !spec.monster {
        return false;
    }

    let result = dispatch(spec, line.clone(), thing, level);

    if result {
        change_switch_texture(
            line,
            spec.trigger.is_repeatable(),
            &level.switch_list,
            &mut level.button_list,
            &level.snd_command,
            &mut level.level_data.bsp_3d,
        );
    }

    result
}

fn dispatch(
    spec: DecodedSpecial,
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
                ev_vertical_door(line, thing, kind, spec.trigger.is_repeatable(), level);
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
    use crate::env::switch::p_use_special_line;
    use crate::test_support::{TestLevel, rng_guard};
    use level::special_encode::encode_vanilla;

    /// Locked doors refuse without the key and open with it, in both the
    /// manual (26, PD_BLUEK) and key switch (137, PD_YELLOWO with a skull
    /// accepted for a card) forms — the lock bits through the real
    /// decode + dispatch path. The demo maps barely exercise locks.
    #[test]
    fn locked_doors_enforce_keys() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        let player = level.spawn_player(1056, -3616);

        // Any manual door line will do (found by its pre-normalisation
        // special); relock it blue.
        let ld_idx = level
            .level_data()
            .linedefs
            .iter()
            .position(|l| l.default_special == 1)
            .expect("E1M1 has a manual door");
        let mut ld = MapPtr::new(&mut level.level_data_mut().linedefs[ld_idx]);
        ld.special = encode_vanilla(26).unwrap();

        let used = p_use_special_line(0, ld.clone(), unsafe { &mut *player });
        assert!(used, "vanilla counts a failed key check as used");
        assert!(ld.backsector.as_ref().unwrap().specialdata.is_none());
        assert_eq!(level.players()[0].message, Some(PD_BLUEK));

        level.parts_mut().1[0].status.cards[Card::Bluecard as usize] = true;
        p_use_special_line(0, ld.clone(), unsafe { &mut *player });
        assert!(
            ld.backsector.as_ref().unwrap().specialdata.is_some(),
            "manual door must open with the blue card"
        );

        // Switch form on ld195 (tag 2): yellow once-only blaze open.
        let mut sw = MapPtr::new(&mut level.level_data_mut().linedefs[195]);
        sw.special = encode_vanilla(137).unwrap();
        p_use_special_line(0, sw.clone(), unsafe { &mut *player });
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
        p_use_special_line(0, sw.clone(), unsafe { &mut *player });
        assert!(
            tagged(&level).iter().all(|d| *d),
            "yellow skull must open the yellow card switch-door"
        );
    }
}
