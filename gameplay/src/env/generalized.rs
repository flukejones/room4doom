/// BOOM generalized linedef types (0x2F80–0x7FFF)
///
/// Each category encodes its behavior in bit fields within the special number.
/// Common trigger bits (0-2) determine activation method.
use crate::doom_def::Card;
use crate::env::ceiling::{CeilKind, ev_do_ceiling};
use crate::env::doors::{DoorKind, ev_do_door};
use crate::env::floor::{FloorKind, StairKind, ev_build_stairs, ev_do_floor};
use crate::env::platforms::{PlatKind, ev_do_platform};
use crate::env::switch::{change_switch_texture, start_sector_sound};
use crate::lang::english::*;
use crate::level::LevelState;
use crate::thing::MapObject;
use level::MapPtr;
use level::map_defs::LineDef;
use log::debug;
use sound_common::SfxName;

// Category base addresses
const GEN_CRUSHER_BASE: i16 = 0x2F80;
const GEN_STAIRS_BASE: i16 = 0x3000;
const GEN_LIFT_BASE: i16 = 0x3400;
const GEN_LOCKED_BASE: i16 = 0x3800;
const GEN_DOOR_BASE: i16 = 0x3C00;
const GEN_CEILING_BASE: i16 = 0x4000;
const GEN_FLOOR_BASE: i16 = 0x6000;

// Trigger type (bits 0-2)
const TRIGGER_MASK: i16 = 0x0007;

#[derive(Debug, Clone, Copy, PartialEq)]
enum TriggerType {
    WalkOnce,
    WalkMany,
    SwitchOnce,
    SwitchMany,
    GunOnce,
    GunMany,
    PushOnce,
    PushMany,
}

impl TriggerType {
    fn from_bits(val: i16) -> Self {
        match val & TRIGGER_MASK {
            0 => TriggerType::WalkOnce,
            1 => TriggerType::WalkMany,
            2 => TriggerType::SwitchOnce,
            3 => TriggerType::SwitchMany,
            4 => TriggerType::GunOnce,
            5 => TriggerType::GunMany,
            6 => TriggerType::PushOnce,
            7 => TriggerType::PushMany,
            _ => unreachable!(),
        }
    }

    fn is_walk(self) -> bool {
        matches!(self, TriggerType::WalkOnce | TriggerType::WalkMany)
    }

    fn is_switch(self) -> bool {
        matches!(self, TriggerType::SwitchOnce | TriggerType::SwitchMany)
    }

    fn is_gun(self) -> bool {
        matches!(self, TriggerType::GunOnce | TriggerType::GunMany)
    }

    fn is_push(self) -> bool {
        matches!(self, TriggerType::PushOnce | TriggerType::PushMany)
    }

    fn is_repeatable(self) -> bool {
        matches!(
            self,
            TriggerType::WalkMany
                | TriggerType::SwitchMany
                | TriggerType::GunMany
                | TriggerType::PushMany
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Category {
    Crusher,
    Stairs,
    Lift,
    LockedDoor,
    Door,
    Ceiling,
    Floor,
}

pub fn is_generalized(special: i16) -> bool {
    special >= GEN_CRUSHER_BASE
}

fn categorize(special: i16) -> Option<Category> {
    if special >= GEN_FLOOR_BASE {
        Some(Category::Floor)
    } else if special >= GEN_CEILING_BASE {
        Some(Category::Ceiling)
    } else if special >= GEN_DOOR_BASE {
        Some(Category::Door)
    } else if special >= GEN_LOCKED_BASE {
        Some(Category::LockedDoor)
    } else if special >= GEN_LIFT_BASE {
        Some(Category::Lift)
    } else if special >= GEN_STAIRS_BASE {
        Some(Category::Stairs)
    } else if special >= GEN_CRUSHER_BASE {
        Some(Category::Crusher)
    } else {
        None
    }
}

/// Entry point for walk-triggered generalized linedefs
pub fn handle_generalized_cross(
    mut line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
    is_monster: bool,
) -> bool {
    let special = line.special;
    let trigger = TriggerType::from_bits(special);

    if !trigger.is_walk() {
        return false;
    }

    let Some(category) = categorize(special) else {
        return false;
    };

    if is_monster && !gen_allows_monster(special, category) {
        return false;
    }

    let result = dispatch(special, category, line.clone(), thing, level);

    if result && !trigger.is_repeatable() {
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
    let special = line.special;
    let trigger = TriggerType::from_bits(special);

    if !trigger.is_switch() && !trigger.is_push() {
        return false;
    }

    let Some(category) = categorize(special) else {
        return false;
    };

    let result = dispatch(special, category, line.clone(), thing, level);

    if result {
        let repeatable = trigger.is_repeatable();
        change_switch_texture(
            line,
            repeatable,
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
    let special = line.special;
    let trigger = TriggerType::from_bits(special);

    if !trigger.is_gun() {
        return false;
    }

    let Some(category) = categorize(special) else {
        return false;
    };

    let result = dispatch(special, category, line.clone(), thing, level);

    if result {
        let repeatable = trigger.is_repeatable();
        change_switch_texture(
            line,
            repeatable,
            &level.switch_list,
            &mut level.button_list,
            &level.snd_command,
            &mut level.level_data.bsp_3d,
        );
    }

    result
}

fn dispatch(
    special: i16,
    category: Category,
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    match category {
        Category::Floor => dispatch_floor(special, line, level),
        Category::Ceiling => dispatch_ceiling(special, line, level),
        Category::Door => dispatch_door(special, line, level),
        Category::LockedDoor => dispatch_locked_door(special, line, thing, level),
        Category::Lift => dispatch_lift(special, line, level),
        Category::Stairs => dispatch_stairs(special, line, level),
        Category::Crusher => dispatch_crusher(special, line, level),
    }
}

/// Check if the generalized linedef allows monster activation
fn gen_allows_monster(special: i16, category: Category) -> bool {
    match category {
        Category::Floor => {
            let val = (special - GEN_FLOOR_BASE) as u16;
            val & (1 << 5) != 0
        }
        Category::Ceiling => {
            let val = (special - GEN_CEILING_BASE) as u16;
            val & (1 << 5) != 0
        }
        Category::Door => {
            let val = (special - GEN_DOOR_BASE) as u16;
            val & (1 << 4) != 0
        }
        Category::LockedDoor => {
            let val = (special - GEN_LOCKED_BASE) as u16;
            val & (1 << 4) != 0
        }
        Category::Lift => {
            let val = (special - GEN_LIFT_BASE) as u16;
            val & (1 << 5) != 0
        }
        Category::Stairs => {
            let val = (special - GEN_STAIRS_BASE) as u16;
            val & (1 << 5) != 0
        }
        Category::Crusher => {
            let val = (special - GEN_CRUSHER_BASE) as u16;
            val & (1 << 5) != 0
        }
    }
}

// Floor: bits 3-4 speed, bit 5 monster, bit 6 model, bits 7-9 target,
//        bit 10 direction, bits 11-12 change, bit 13 crush
fn dispatch_floor(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_FLOOR_BASE) as u16;
    let speed = (val >> 3) & 0x03;
    let direction = (val >> 10) & 0x01;
    let target = (val >> 7) & 0x07;
    let crush = (val >> 13) & 0x01;

    debug!(
        "Generalized floor: speed={}, dir={}, target={}, crush={}",
        speed, direction, target, crush
    );

    let kind = match (direction, target) {
        (0, 0) => FloorKind::LowerFloorToLowest,
        (0, 1) => FloorKind::LowerFloorToLowest,
        (0, 2) => FloorKind::LowerFloorToLowest,
        (0, _) => FloorKind::LowerFloor,
        (_, 0) => FloorKind::RaiseFloorToNearest,
        (_, 1) => FloorKind::RaiseFloorToNearest,
        (_, 2) => FloorKind::RaiseFloorToNearest,
        (_, 5) => FloorKind::RaiseFloor24,
        (..) => FloorKind::RaiseFloor,
    };

    ev_do_floor(line, kind, level)
}

// Ceiling: bits 3-4 speed, bit 5 monster, bit 6 model, bits 7-9 target,
//          bit 10 direction, bits 11-12 change, bit 13 crush
fn dispatch_ceiling(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_CEILING_BASE) as u16;
    let direction = (val >> 10) & 0x01;
    let crush = (val >> 13) & 0x01;

    debug!("Generalized ceiling: dir={}, crush={}", direction, crush);

    let kind = if crush != 0 {
        CeilKind::CrushAndRaise
    } else if direction == 0 {
        CeilKind::LowerToFloor
    } else {
        CeilKind::RaiseToHighest
    };

    ev_do_ceiling(line, kind, level)
}

// Door: bits 3-4 speed, bit 4 monster, bits 5-6 kind, bits 7-8 delay
fn dispatch_door(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_DOOR_BASE) as u16;
    let speed = (val >> 3) & 0x03;
    let door_kind_bits = (val >> 5) & 0x03;

    debug!("Generalized door: speed={}, kind={}", speed, door_kind_bits);

    let kind = match door_kind_bits {
        0 => {
            if speed >= 2 {
                DoorKind::BlazeRaise
            } else {
                DoorKind::Normal
            }
        }
        1 => {
            if speed >= 2 {
                DoorKind::BlazeOpen
            } else {
                DoorKind::Open
            }
        }
        2 => {
            if speed >= 2 {
                DoorKind::BlazeClose
            } else {
                DoorKind::Close
            }
        }
        3 => DoorKind::Close30ThenOpen,
        _ => DoorKind::Normal,
    };

    ev_do_door(line, kind, level)
}

/// BOOM generalized locked door key types (bits 6-8)
#[derive(Debug, Clone, Copy)]
enum GenLockedKey {
    Any,
    RedCard,
    BlueCard,
    YellowCard,
    RedSkull,
    BlueSkull,
    YellowSkull,
    All,
}

impl GenLockedKey {
    fn from_bits(val: u16) -> Self {
        match (val >> 6) & 0x07 {
            0 => GenLockedKey::Any,
            1 => GenLockedKey::RedCard,
            2 => GenLockedKey::BlueCard,
            3 => GenLockedKey::YellowCard,
            4 => GenLockedKey::RedSkull,
            5 => GenLockedKey::BlueSkull,
            6 => GenLockedKey::YellowSkull,
            7 => GenLockedKey::All,
            _ => GenLockedKey::Any,
        }
    }
}

// Locked door: bits 3-4 speed, bit 4 monster, bits 5 kind, bits 6-8 key, bit 9
// skulliscard
fn dispatch_locked_door(
    special: i16,
    line: MapPtr<LineDef>,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    let val = (special - GEN_LOCKED_BASE) as u16;
    let speed = (val >> 3) & 0x03;
    let door_kind_bits = (val >> 5) & 0x01;
    let key = GenLockedKey::from_bits(val);
    // Bit 9: when set, cards and skulls are interchangeable (like vanilla Doom).
    // When clear, must have the exact key type specified.
    let skull_is_card = (val >> 9) & 0x01 != 0;

    debug!(
        "Generalized locked door: speed={}, kind={}, key={:?}, skull_is_card={}",
        speed, door_kind_bits, key, skull_is_card
    );

    let Some(player) = thing.player_mut() else {
        return false;
    };

    let cards = &player.status.cards;

    let has_key = match key {
        GenLockedKey::Any => {
            cards[Card::Redcard as usize]
                || cards[Card::Redskull as usize]
                || cards[Card::Bluecard as usize]
                || cards[Card::Blueskull as usize]
                || cards[Card::Yellowcard as usize]
                || cards[Card::Yellowskull as usize]
        }
        GenLockedKey::RedCard => {
            cards[Card::Redcard as usize] || (skull_is_card && cards[Card::Redskull as usize])
        }
        GenLockedKey::BlueCard => {
            cards[Card::Bluecard as usize] || (skull_is_card && cards[Card::Blueskull as usize])
        }
        GenLockedKey::YellowCard => {
            cards[Card::Yellowcard as usize] || (skull_is_card && cards[Card::Yellowskull as usize])
        }
        GenLockedKey::RedSkull => {
            cards[Card::Redskull as usize] || (skull_is_card && cards[Card::Redcard as usize])
        }
        GenLockedKey::BlueSkull => {
            cards[Card::Blueskull as usize] || (skull_is_card && cards[Card::Bluecard as usize])
        }
        GenLockedKey::YellowSkull => {
            cards[Card::Yellowskull as usize] || (skull_is_card && cards[Card::Yellowcard as usize])
        }
        GenLockedKey::All if skull_is_card => {
            (cards[Card::Redcard as usize] || cards[Card::Redskull as usize])
                && (cards[Card::Bluecard as usize] || cards[Card::Blueskull as usize])
                && (cards[Card::Yellowcard as usize] || cards[Card::Yellowskull as usize])
        }
        GenLockedKey::All => {
            cards[Card::Redcard as usize]
                && cards[Card::Redskull as usize]
                && cards[Card::Bluecard as usize]
                && cards[Card::Blueskull as usize]
                && cards[Card::Yellowcard as usize]
                && cards[Card::Yellowskull as usize]
        }
    };

    if !has_key {
        let msg = match key {
            GenLockedKey::RedCard => {
                if skull_is_card {
                    PD_REDK
                } else {
                    PD_REDC
                }
            }
            GenLockedKey::BlueCard => {
                if skull_is_card {
                    PD_BLUEK
                } else {
                    PD_BLUEC
                }
            }
            GenLockedKey::YellowCard => {
                if skull_is_card {
                    PD_YELLOWK
                } else {
                    PD_YELLOWC
                }
            }
            GenLockedKey::RedSkull => {
                if skull_is_card {
                    PD_REDK
                } else {
                    PD_REDS
                }
            }
            GenLockedKey::BlueSkull => {
                if skull_is_card {
                    PD_BLUEK
                } else {
                    PD_BLUES
                }
            }
            GenLockedKey::YellowSkull => {
                if skull_is_card {
                    PD_YELLOWK
                } else {
                    PD_YELLOWS
                }
            }
            GenLockedKey::All => {
                if skull_is_card {
                    PD_ALL3
                } else {
                    PD_ALL6
                }
            }
            GenLockedKey::Any => PD_ANY,
        };
        player.message = Some(msg);
        start_sector_sound(&line, SfxName::Oof, &level.snd_command);
        return false;
    }

    let kind = if speed >= 2 {
        if door_kind_bits == 0 {
            DoorKind::BlazeRaise
        } else {
            DoorKind::BlazeOpen
        }
    } else {
        if door_kind_bits == 0 {
            DoorKind::Normal
        } else {
            DoorKind::Open
        }
    };

    ev_do_door(line, kind, level)
}

// Lift: bits 3-4 speed, bit 5 monster, bits 6-7 delay, bits 8-9 target
fn dispatch_lift(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_LIFT_BASE) as u16;
    let target = (val >> 8) & 0x03;

    debug!("Generalized lift: target={}", target);

    let kind = match target {
        0 => PlatKind::DownWaitUpStay,
        1 => PlatKind::DownWaitUpStay,
        2 => PlatKind::DownWaitUpStay,
        3 => PlatKind::PerpetualRaise,
        _ => PlatKind::DownWaitUpStay,
    };

    ev_do_platform(line, kind, 0, level)
}

// Stairs: bits 3-4 speed, bit 5 monster, bit 6 ignore, bit 7 direction, bits
// 8-9 step
fn dispatch_stairs(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_STAIRS_BASE) as u16;
    let speed = (val >> 3) & 0x03;

    debug!("Generalized stairs: speed={}", speed);

    let kind = if speed >= 2 {
        StairKind::Turbo16
    } else {
        StairKind::Build8
    };

    ev_build_stairs(line, kind, level)
}

// Crusher: bits 3-4 speed, bit 5 monster, bit 6 silent
fn dispatch_crusher(special: i16, line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let val = (special - GEN_CRUSHER_BASE) as u16;
    let silent = (val >> 6) & 0x01;
    let speed = (val >> 3) & 0x03;

    debug!("Generalized crusher: speed={}, silent={}", speed, silent);

    let kind = if silent != 0 {
        CeilKind::SilentCrushAndRaise
    } else if speed >= 2 {
        CeilKind::FastCrushAndRaise
    } else {
        CeilKind::CrushAndRaise
    };

    ev_do_ceiling(line, kind, level)
}
