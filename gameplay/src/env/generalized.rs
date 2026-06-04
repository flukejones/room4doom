//! BOOM generalized + engine-extended linedef dispatch.
//!
//! All mover specials are normalised to generalized form at load (see
//! [`level::special_encode`]); these entry points decode them to the exact
//! room4doom mover kind and call the relevant `ev_do_*`.
use crate::env::ceiling::{CeilKind, ev_do_ceiling};
use crate::env::doors::{DoorKind, ev_do_door, ev_vertical_door};
use crate::env::floor::{FloorKind, StairKind, ev_build_stairs, ev_do_donut, ev_do_floor};
use crate::env::platforms::{PlatKind, ev_do_platform};
use crate::env::switch::change_switch_texture;
use crate::level::LevelState;
use crate::thing::MapObject;
use level::MapPtr;
use level::map_defs::LineDef;
use level::special_encode::{self, Category as SpecialCategory, Trigger as SpecialTrigger};

// BOOM category base addresses (for the monster-bit check).
const GEN_CRUSHER_BASE: u32 = 0x2F80;
const GEN_STAIRS_BASE: u32 = 0x3000;
const GEN_LIFT_BASE: u32 = 0x3400;
const GEN_DOOR_BASE: u32 = 0x3C00;
const GEN_CEILING_BASE: u32 = 0x4000;
const GEN_FLOOR_BASE: u32 = 0x6000;

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

fn trigger_class(t: SpecialTrigger) -> TriggerClass {
    match t {
        SpecialTrigger::WalkOnce | SpecialTrigger::WalkMany => TriggerClass::Walk,
        SpecialTrigger::SwitchOnce | SpecialTrigger::SwitchMany => TriggerClass::Switch,
        SpecialTrigger::GunOnce | SpecialTrigger::GunMany => TriggerClass::Gun,
        SpecialTrigger::PushOnce | SpecialTrigger::PushMany => TriggerClass::Push,
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
    if is_monster && !gen_allows_monster(line.special, spec.category) {
        return false;
    }

    let result = dispatch(line.special, line.clone(), thing, level);

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

    let result = dispatch(line.special, line.clone(), thing, level);

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

    let result = dispatch(line.special, line.clone(), thing, level);

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
    special: u32,
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    let Some(spec) = special_encode::decode(special) else {
        return false;
    };
    match spec.category {
        SpecialCategory::Floor => {
            if spec.composite {
                let ceil = ev_do_ceiling(line.clone(), CeilKind::RaiseToHighest, level);
                let floor = ev_do_floor(line, FloorKind::LowerFloorToLowest, level);
                return ceil || floor;
            }
            let Ok(kind) = FloorKind::try_from(spec.kind) else {
                return false;
            };
            if matches!(kind, FloorKind::DonutRaise) {
                return ev_do_donut(line, level);
            }
            ev_do_floor(line, kind, level)
        }
        SpecialCategory::Ceiling => match CeilKind::try_from(spec.kind) {
            Ok(kind) => ev_do_ceiling(line, kind, level),
            Err(_) => false,
        },
        SpecialCategory::Door => {
            if spec.manual {
                ev_vertical_door(line, thing, level);
                return true;
            }
            match DoorKind::try_from(spec.kind) {
                Ok(kind) => ev_do_door(line, kind, level),
                Err(_) => false,
            }
        }
        SpecialCategory::Lift => match PlatKind::try_from(spec.kind) {
            Ok(kind) => ev_do_platform(line, kind, spec.amount, level),
            Err(_) => false,
        },
        SpecialCategory::Stairs => match StairKind::try_from(spec.kind) {
            Ok(kind) => ev_build_stairs(line, kind, level),
            Err(_) => false,
        },
    }
}

/// Check if a standard-BOOM generalized linedef allows monster activation.
/// Extended specials are never monster-activated (return false).
fn gen_allows_monster(special: u32, category: SpecialCategory) -> bool {
    if special_encode::is_extended(special) {
        return false;
    }
    // Door monster bit is bit 7; floor/ceiling/lift/stairs/crusher use bit 5.
    match category {
        SpecialCategory::Door => (special - GEN_DOOR_BASE) & (1 << 7) != 0,
        SpecialCategory::Floor => (special - GEN_FLOOR_BASE) & (1 << 5) != 0,
        SpecialCategory::Lift => (special - GEN_LIFT_BASE) & (1 << 5) != 0,
        SpecialCategory::Stairs => (special - GEN_STAIRS_BASE) & (1 << 5) != 0,
        SpecialCategory::Ceiling => {
            // crusher (0x2F80) and ceiling (0x4000) both use bit 5.
            let base = if special >= GEN_CEILING_BASE {
                GEN_CEILING_BASE
            } else {
                GEN_CRUSHER_BASE
            };
            (special - base) & (1 << 5) != 0
        }
    }
}
