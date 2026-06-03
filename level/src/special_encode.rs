//! Canonical mover encoding: at level load, rewrite every vanilla (Doom 1-141)
//! mover linedef special into a generalized form so the engine has one decode
//! path for movers.
//!
//! Two target forms, both decoded by the same generalized handler:
//!
//! - **Standard BOOM** (`0x2F80..=0x7FFF`): when a vanilla special's behaviour
//!   is exactly expressible in BOOM's generalized bitfields (speed/model/
//!   target/direction/change/crush/delay), it is rewritten to the real BOOM
//!   special. Hand-authored BOOM maps already use this range and decode
//!   identically. Each value is built from the field constants below, never
//!   hand-typed, so the bit packing is correct by construction.
//! - **Extended** (`>= EXT_BASE`, the otherwise-unused high `u32` range): for
//!   kinds BOOM cannot name exactly — donut, the composite vanilla-40, manual
//!   doors (ev_vertical_door), the key switch-doors, RaiseFloor512, the
//!   platform texture-change kinds, and `lowerAndCrush` (OG leaves crush off,
//!   which BOOM's crusher cannot express). The extended form carries the exact
//!   mover identity in explicit fields.
//!
//! Behaviour is verified against OG Doom C (the demo-safety oracle) cross-
//! checked with BOOM 2.02 P_GENLIN.C / P_SPEC.H. `encode_vanilla` is idempotent
//! and only rewrites vanilla numbers in `1..=141`.

/// Lowest standard BOOM generalized special.
pub const GEN_MIN: u32 = 0x2F80;
/// Highest standard BOOM generalized special (16-bit positive max).
pub const GEN_MAX: u32 = 0x7FFF;
/// Base of the engine-extended namespace (above all 16-bit specials).
pub const EXT_BASE: u32 = 0x0001_0000;

// --- Standard BOOM category bases (Boom 2.02 P_SPEC.H) ------------------
pub const GEN_CRUSHER_BASE: u32 = 0x2F80;
pub const GEN_STAIRS_BASE: u32 = 0x3000;
pub const GEN_LIFT_BASE: u32 = 0x3400;
pub const GEN_LOCKED_BASE: u32 = 0x3800;
pub const GEN_DOOR_BASE: u32 = 0x3C00;
pub const GEN_CEILING_BASE: u32 = 0x4000;
pub const GEN_FLOOR_BASE: u32 = 0x6000;

// --- BOOM field shifts (P_SPEC.H) --------------------------------------
const TRIGGER_SHIFT: u32 = 0;
const FC_SPEED_SHIFT: u32 = 3;
const FC_MODEL_SHIFT: u32 = 5;
const FC_DIR_SHIFT: u32 = 6;
const FC_TARGET_SHIFT: u32 = 7;
const FC_CHANGE_SHIFT: u32 = 10;
const FC_CRUSH_SHIFT: u32 = 12;
const DOOR_SPEED_SHIFT: u32 = 3;
const DOOR_KIND_SHIFT: u32 = 5;
const DOOR_DELAY_SHIFT: u32 = 8;
const LIFT_SPEED_SHIFT: u32 = 3;
const LIFT_DELAY_SHIFT: u32 = 6;
const LIFT_TARGET_SHIFT: u32 = 8;
const STAIR_SPEED_SHIFT: u32 = 3;
const STAIR_STEP_SHIFT: u32 = 6;
const STAIR_DIR_SHIFT: u32 = 8;
const CRUSHER_SPEED_SHIFT: u32 = 3;
const CRUSHER_SILENT_SHIFT: u32 = 6;

// BOOM speed tiers.
const SPEED_SLOW: u32 = 0;
const SPEED_NORMAL: u32 = 1;
const SPEED_FAST: u32 = 2;
const SPEED_TURBO: u32 = 3;
// BOOM floor targets.
const FTO_HNF: u32 = 1; // highest neighbour floor
const FTO_LNF: u32 = 2; // lowest neighbour floor
const FTO_NNF: u32 = 3; // next-highest neighbour floor
const FTO_LNC: u32 = 4; // lowest neighbour ceiling
const FBY_ST: u32 = 5; // by shortest lower texture
const FBY_24: u32 = 6; // by 24
// BOOM ceiling target.
const CTO_F: u32 = 4; // to own floor
// BOOM change: texture only, keep type.
const CHG_TXT: u32 = 2;
// BOOM lift targets.
const F2_LNF: u32 = 0; // down to lowest neighbour floor
const LNF_2_HNF: u32 = 3; // perpetual: lowest..highest
// BOOM door kinds.
const DK_ODC: u32 = 0; // open-delay-close (raise)
const DK_O: u32 = 1; // open and stay
const DK_CDO: u32 = 2; // close-delay-open
const DK_C: u32 = 3; // close and stay
// BOOM door delay tiers: tier1 = VDOORWAIT (150t), tier3 = 7*VDOORWAIT (1050t).
const DELAY_T1: u32 = 1;
const DELAY_T3: u32 = 3;
// BOOM lift delay tier1 = PLATWAIT (105t).
const LIFT_DELAY_T1: u32 = 1;
// BOOM stair step size: 1 => 8 units, 2 => 16 units.
const STEP_8: u32 = 1;
const STEP_16: u32 = 2;
const STAIR_UP: u32 = 1;

/// Trigger classes (activation × once/repeatable), low 3 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    WalkOnce,
    WalkMany,
    SwitchOnce,
    SwitchMany,
    GunOnce,
    GunMany,
    PushOnce,
    PushMany,
}

impl Trigger {
    const fn bits(self) -> u32 {
        self as u32
    }
    pub fn is_repeatable(self) -> bool {
        matches!(
            self,
            Trigger::WalkMany | Trigger::SwitchMany | Trigger::GunMany | Trigger::PushMany
        )
    }
}

/// Mover category: which `ev_do_*` entry the special drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Floor,
    Ceiling,
    Door,
    Lift,
    Stairs,
}

// --- Extended namespace layout (bits above EXT_BASE) --------------------
// bits 0-2 category, 3-5 trigger, 6-9 kind, 10-11 amount (0/24/32),
// 12 manual, 13 composite.
const EXT_AMOUNT_SHIFT: u32 = 10;
const EXT_MANUAL_BIT: u32 = 1 << 12;
const EXT_COMPOSITE_BIT: u32 = 1 << 13;

impl Category {
    const fn ext_bits(self) -> u32 {
        self as u32
    }
    fn from_ext_bits(v: u32) -> Option<Category> {
        match v & 0x7 {
            0 => Some(Category::Floor),
            1 => Some(Category::Ceiling),
            2 => Some(Category::Door),
            3 => Some(Category::Lift),
            4 => Some(Category::Stairs),
            _ => None,
        }
    }
}

fn trigger_from_bits(v: u32) -> Trigger {
    match v & 0x7 {
        0 => Trigger::WalkOnce,
        1 => Trigger::WalkMany,
        2 => Trigger::SwitchOnce,
        3 => Trigger::SwitchMany,
        4 => Trigger::GunOnce,
        5 => Trigger::GunMany,
        6 => Trigger::PushOnce,
        _ => Trigger::PushMany,
    }
}

/// Decoded identity of an extended mover special.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtSpec {
    pub category: Category,
    pub trigger: Trigger,
    /// Kind enum discriminant; interpret per `category`.
    pub kind: u8,
    /// Platform RaiseAndChange height (0, 24 or 32).
    pub amount: i32,
    /// Door routes through `ev_vertical_door`.
    pub manual: bool,
    /// Floor also fires ceiling RaiseToHighest + floor LowerFloorToLowest.
    pub composite: bool,
}

/// True if `special` is an engine-extended mover special.
pub fn is_extended(special: u32) -> bool {
    special >= EXT_BASE
}

/// True if `special` is any generalized mover (standard BOOM or extended).
pub fn is_generalized(special: u32) -> bool {
    (GEN_MIN..=GEN_MAX).contains(&special) || special >= EXT_BASE
}

const fn ext(cat: Category, trig: Trigger, kind: u8) -> u32 {
    EXT_BASE | cat.ext_bits() | (trig.bits() << 3) | ((kind as u32) << 6)
}
const fn with_amount(s: u32, amount: i32) -> u32 {
    let a = match amount {
        24 => 1,
        32 => 2,
        _ => 0,
    };
    s | (a << EXT_AMOUNT_SHIFT)
}
const fn with_manual(s: u32) -> u32 {
    s | EXT_MANUAL_BIT
}
const fn with_composite(s: u32) -> u32 {
    s | EXT_COMPOSITE_BIT
}

/// Decode an extended special. `None` for standard BOOM and vanilla specials.
pub fn decode_ext(special: u32) -> Option<ExtSpec> {
    if !is_extended(special) {
        return None;
    }
    let v = special - EXT_BASE;
    let category = Category::from_ext_bits(v)?;
    let trigger = trigger_from_bits(v >> 3);
    let kind = ((v >> 6) & 0xF) as u8;
    let amount = match (v >> EXT_AMOUNT_SHIFT) & 0x3 {
        1 => 24,
        2 => 32,
        _ => 0,
    };
    Some(ExtSpec {
        category,
        trigger,
        kind,
        amount,
        manual: (v & EXT_MANUAL_BIT) != 0,
        composite: (v & EXT_COMPOSITE_BIT) != 0,
    })
}

// room4doom kind discriminants (enum #[repr(u8)] order), used by the decoder.
const FK_LOWER: u8 = 0;
const FK_LOWEST: u8 = 1;
const FK_TURBO_LOWER: u8 = 2;
const FK_RAISE: u8 = 3;
const FK_RAISE_NEAREST: u8 = 4;
const FK_TO_TEXTURE: u8 = 5;
const FK_LOWER_CHANGE: u8 = 6;
const FK_RAISE24: u8 = 7;
const FK_RAISE24_CHANGE: u8 = 8;
const FK_RAISE_CRUSH: u8 = 9;
const FK_RAISE_TURBO: u8 = 10;
const CK_LOWER_TO_FLOOR: u8 = 0;
const CK_CRUSH: u8 = 3;
const CK_FAST_CRUSH: u8 = 4;
const CK_SILENT_CRUSH: u8 = 5;
const DK_NORMAL: u8 = 0;
const DK_CLOSE: u8 = 2;
const DK_OPEN: u8 = 3;
const DK_BLAZE_RAISE: u8 = 5;
const DK_BLAZE_OPEN: u8 = 6;
const DK_BLAZE_CLOSE: u8 = 7;
const PK_PERPETUAL: u8 = 0;
const PK_DWUS: u8 = 1;
const PK_BLAZE_DWUS: u8 = 4;
const SK_BUILD8: u8 = 0;
const SK_TURBO16: u8 = 1;

/// Resolve a standard BOOM special to its category by base address.
fn boom_category(special: u32) -> Option<Category> {
    if special >= GEN_FLOOR_BASE {
        Some(Category::Floor)
    } else if special >= GEN_CEILING_BASE {
        Some(Category::Ceiling)
    } else if special >= GEN_DOOR_BASE {
        Some(Category::Door)
    } else if special >= GEN_LOCKED_BASE {
        Some(Category::Door) // generalized locked door -> door category
    } else if special >= GEN_LIFT_BASE {
        Some(Category::Lift)
    } else if special >= GEN_STAIRS_BASE {
        Some(Category::Stairs)
    } else if special >= GEN_CRUSHER_BASE {
        Some(Category::Ceiling)
    } else {
        None
    }
}

/// Decode any generalized mover special (standard BOOM or extended) into the
/// room4doom mover identity the engine dispatch and tooling act on. Returns
/// `None` for non-generalized specials.
///
/// For standard BOOM, the bitfields are mapped to the EXACT room4doom kind
/// (the inverse of `encode_vanilla`), so decoding an engine-rewritten vanilla
/// special reproduces OG behaviour. Hand-authored BOOM specials decode the same
/// way.
pub fn decode(special: u32) -> Option<ExtSpec> {
    if is_extended(special) {
        return decode_ext(special);
    }
    if !(GEN_MIN..=GEN_MAX).contains(&special) {
        return None;
    }
    let category = boom_category(special)?;
    let trigger = trigger_from_bits(special >> TRIGGER_SHIFT);
    let kind = match category {
        Category::Floor => decode_floor_kind(special - GEN_FLOOR_BASE),
        Category::Ceiling => decode_ceiling_kind(special),
        Category::Door => decode_door_kind(special - GEN_DOOR_BASE),
        Category::Lift => decode_lift_kind(special - GEN_LIFT_BASE),
        Category::Stairs => decode_stair_kind(special - GEN_STAIRS_BASE),
    };
    Some(ExtSpec {
        category,
        trigger,
        kind,
        amount: 0,
        manual: false,
        composite: false,
    })
}

fn field(v: u32, shift: u32, mask: u32) -> u32 {
    (v >> shift) & mask
}

fn decode_floor_kind(v: u32) -> u8 {
    let speed = field(v, FC_SPEED_SHIFT, 0x3);
    let dir = field(v, FC_DIR_SHIFT, 0x1);
    let target = field(v, FC_TARGET_SHIFT, 0x7);
    let change = field(v, FC_CHANGE_SHIFT, 0x3);
    let crush = field(v, FC_CRUSH_SHIFT, 0x1);
    let fast = speed >= SPEED_FAST;
    if dir == 0 {
        // down
        match (target, change) {
            (FTO_LNF, CHG_TXT) => FK_LOWER_CHANGE,
            (FTO_LNF, _) => FK_LOWEST,
            (FTO_HNF, _) if fast => FK_TURBO_LOWER,
            (FTO_HNF, _) => FK_LOWER,
            _ => FK_LOWER,
        }
    } else {
        // up
        if crush == 1 {
            return FK_RAISE_CRUSH;
        }
        match (target, change, fast) {
            (FTO_LNC, ..) => FK_RAISE,
            (FBY_ST, ..) => FK_TO_TEXTURE,
            (FBY_24, CHG_TXT, _) => FK_RAISE24_CHANGE,
            (FBY_24, ..) => FK_RAISE24,
            (FTO_NNF, _, true) => FK_RAISE_TURBO,
            (FTO_NNF, _, false) => FK_RAISE_NEAREST,
            _ => FK_RAISE,
        }
    }
}

fn decode_ceiling_kind(special: u32) -> u8 {
    if special >= GEN_CEILING_BASE {
        // GenCeiling: only LowerToFloor used (CtoF, down).
        CK_LOWER_TO_FLOOR
    } else {
        // GenCrusher.
        let v = special - GEN_CRUSHER_BASE;
        let speed = field(v, CRUSHER_SPEED_SHIFT, 0x3);
        let silent = field(v, CRUSHER_SILENT_SHIFT, 0x1);
        if silent == 1 {
            CK_SILENT_CRUSH
        } else if speed >= SPEED_NORMAL {
            CK_FAST_CRUSH
        } else {
            CK_CRUSH
        }
    }
}

fn decode_door_kind(v: u32) -> u8 {
    let speed = field(v, DOOR_SPEED_SHIFT, 0x3);
    let kind = field(v, DOOR_KIND_SHIFT, 0x3);
    let fast = speed >= SPEED_FAST;
    match (kind, fast) {
        (DK_ODC, false) => DK_NORMAL,
        (DK_ODC, true) => DK_BLAZE_RAISE,
        (DK_O, false) => DK_OPEN,
        (DK_O, true) => DK_BLAZE_OPEN,
        (DK_C, false) => DK_CLOSE,
        (DK_C, true) => DK_BLAZE_CLOSE,
        // DK_CDO (close30ThenOpen) is encoded extended, never reaches here.
        _ => DK_NORMAL,
    }
}

fn decode_lift_kind(v: u32) -> u8 {
    let speed = field(v, LIFT_SPEED_SHIFT, 0x3);
    let target = field(v, LIFT_TARGET_SHIFT, 0x3);
    if target == LNF_2_HNF {
        PK_PERPETUAL
    } else if speed >= SPEED_TURBO {
        PK_BLAZE_DWUS
    } else {
        PK_DWUS
    }
}

fn decode_stair_kind(v: u32) -> u8 {
    let speed = field(v, STAIR_SPEED_SHIFT, 0x3);
    if speed >= SPEED_FAST {
        SK_TURBO16
    } else {
        SK_BUILD8
    }
}

// --- BOOM special builders (compute bit values by construction) ---------
#[allow(clippy::too_many_arguments)]
const fn boom_fc(
    base: u32,
    trig: Trigger,
    speed: u32,
    dir: u32,
    target: u32,
    change: u32,
    crush: u32,
) -> u32 {
    base | trig.bits()
        | (speed << FC_SPEED_SHIFT)
        | (0 << FC_MODEL_SHIFT) // trigger-sector model (vanilla copies front sector)
        | (dir << FC_DIR_SHIFT)
        | (target << FC_TARGET_SHIFT)
        | (change << FC_CHANGE_SHIFT)
        | (crush << FC_CRUSH_SHIFT)
}
const DOOR_MONSTER_BIT: u32 = 1 << 7; // P_SPEC.H DoorMonster = 0x80
const LIFT_MONSTER_BIT: u32 = 1 << 5; // P_SPEC.H LiftMonster = 0x20

const fn boom_door(base: u32, trig: Trigger, speed: u32, kind: u32, delay: u32) -> u32 {
    base | trig.bits()
        | (speed << DOOR_SPEED_SHIFT)
        | (kind << DOOR_KIND_SHIFT)
        | (delay << DOOR_DELAY_SHIFT)
}
const fn boom_lift(base: u32, trig: Trigger, speed: u32, delay: u32, target: u32) -> u32 {
    base | trig.bits()
        | (speed << LIFT_SPEED_SHIFT)
        | (delay << LIFT_DELAY_SHIFT)
        | (target << LIFT_TARGET_SHIFT)
}
const fn boom_stairs(base: u32, trig: Trigger, speed: u32, step: u32, dir: u32) -> u32 {
    base | trig.bits()
        | (speed << STAIR_SPEED_SHIFT)
        | (step << STAIR_STEP_SHIFT)
        | (dir << STAIR_DIR_SHIFT)
}
const fn boom_crusher(base: u32, trig: Trigger, speed: u32, silent: u32) -> u32 {
    base | trig.bits() | (speed << CRUSHER_SPEED_SHIFT) | (silent << CRUSHER_SILENT_SHIFT)
}

/// Rewrite a vanilla mover special (1-141) into its generalized form.
/// `None` for non-mover specials and any already-generalized special.
pub fn encode_vanilla(special: u32) -> Option<u32> {
    if special >= GEN_MIN {
        return None;
    }
    encode_table(special)
}

#[rustfmt::skip]
fn encode_table(special: u32) -> Option<u32> {
    use Category::{Ceiling, Door, Floor, Lift};
    use Trigger::{GunMany, GunOnce, PushMany, SwitchMany, SwitchOnce, WalkMany, WalkOnce};
    // Extended kind discriminants (room4doom enum #[repr(u8)] order).
    const FK_DONUT: u8 = 11;
    const FK_RAISE512: u8 = 12;
    const FK_LOWEST: u8 = 1;
    const CK_LOWER_CRUSH: u8 = 2;
    const DK_NORMAL: u8 = 0;
    const DK_OPEN: u8 = 3;
    const DK_BLAZE_RAISE: u8 = 5;
    const DK_BLAZE_OPEN: u8 = 6;
    const PK_RAISE_CHANGE: u8 = 2;
    const PK_RAISE_NEAREST: u8 = 3;
    let f = GEN_FLOOR_BASE;
    let c = GEN_CEILING_BASE;
    let d = GEN_DOOR_BASE;
    let l = GEN_LIFT_BASE;
    let s = GEN_STAIRS_BASE;
    let cr = GEN_CRUSHER_BASE;

    let v = match special {
        // ============ FLOORS (BOOM-exact) ============
        // LowerFloor: down -> highest neighbour floor.
        19  => boom_fc(f,WalkOnce,   SPEED_SLOW, 0, FTO_HNF, 0, 0),
        83  => boom_fc(f,WalkMany,   SPEED_SLOW, 0, FTO_HNF, 0, 0),
        102 => boom_fc(f,SwitchOnce, SPEED_SLOW, 0, FTO_HNF, 0, 0),
        45  => boom_fc(f,SwitchMany, SPEED_SLOW, 0, FTO_HNF, 0, 0),
        // LowerFloorToLowest: down -> lowest neighbour floor.
        38  => boom_fc(f,WalkOnce,   SPEED_SLOW, 0, FTO_LNF, 0, 0),
        82  => boom_fc(f,WalkMany,   SPEED_SLOW, 0, FTO_LNF, 0, 0),
        23  => boom_fc(f,SwitchOnce, SPEED_SLOW, 0, FTO_LNF, 0, 0),
        60  => boom_fc(f,SwitchMany, SPEED_SLOW, 0, FTO_LNF, 0, 0),
        // RaiseFloor: up -> lowest neighbour ceiling.
        5   => boom_fc(f,WalkOnce,   SPEED_SLOW, 1, FTO_LNC, 0, 0),
        91  => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FTO_LNC, 0, 0),
        101 => boom_fc(f,SwitchOnce, SPEED_SLOW, 1, FTO_LNC, 0, 0),
        64  => boom_fc(f,SwitchMany, SPEED_SLOW, 1, FTO_LNC, 0, 0),
        24  => boom_fc(f,GunOnce,    SPEED_SLOW, 1, FTO_LNC, 0, 0),
        // RaiseToTexture: up -> shortest lower texture.
        30  => boom_fc(f,WalkOnce,   SPEED_SLOW, 1, FBY_ST, 0, 0),
        96  => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FBY_ST, 0, 0),
        // RaiseFloor24.
        92  => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FBY_24, 0, 0),
        // RaiseFloor24andChange.
        59  => boom_fc(f,WalkOnce,   SPEED_SLOW, 1, FBY_24, CHG_TXT, 0),
        93  => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FBY_24, CHG_TXT, 0),
        // LowerAndChange.
        37  => boom_fc(f,WalkOnce,   SPEED_SLOW, 0, FTO_LNF, CHG_TXT, 0),
        84  => boom_fc(f,WalkMany,   SPEED_SLOW, 0, FTO_LNF, CHG_TXT, 0),
        // TurboLower: down fast -> highest neighbour floor (+8 baked in OG/BOOM).
        36  => boom_fc(f,WalkOnce,   SPEED_FAST, 0, FTO_HNF, 0, 0),
        98  => boom_fc(f,WalkMany,   SPEED_FAST, 0, FTO_HNF, 0, 0),
        71  => boom_fc(f,SwitchOnce, SPEED_FAST, 0, FTO_HNF, 0, 0),
        70  => boom_fc(f,SwitchMany, SPEED_FAST, 0, FTO_HNF, 0, 0),
        // RaiseFloorCrush: up -> lowest neighbour ceiling, crush.
        56  => boom_fc(f,WalkOnce,   SPEED_SLOW, 1, FTO_LNC, 0, 1),
        94  => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FTO_LNC, 0, 1),
        55  => boom_fc(f,SwitchOnce, SPEED_SLOW, 1, FTO_LNC, 0, 1),
        65  => boom_fc(f,SwitchMany, SPEED_SLOW, 1, FTO_LNC, 0, 1),
        // RaiseFloorToNearest: up -> next-highest neighbour floor (OG correct).
        119 => boom_fc(f,WalkOnce,   SPEED_SLOW, 1, FTO_NNF, 0, 0),
        128 => boom_fc(f,WalkMany,   SPEED_SLOW, 1, FTO_NNF, 0, 0),
        18  => boom_fc(f,SwitchOnce, SPEED_SLOW, 1, FTO_NNF, 0, 0),
        69  => boom_fc(f,SwitchMany, SPEED_SLOW, 1, FTO_NNF, 0, 0),
        // RaiseFloorTurbo: up fast -> next-highest neighbour floor.
        130 => boom_fc(f,WalkOnce,   SPEED_FAST, 1, FTO_NNF, 0, 0),
        129 => boom_fc(f,WalkMany,   SPEED_FAST, 1, FTO_NNF, 0, 0),
        131 => boom_fc(f,SwitchOnce, SPEED_FAST, 1, FTO_NNF, 0, 0),
        132 => boom_fc(f,SwitchMany, SPEED_FAST, 1, FTO_NNF, 0, 0),
        // ---- Floors (extended) ----
        140 => ext(Floor, SwitchOnce, FK_RAISE512),
        9   => ext(Floor, SwitchOnce, FK_DONUT),
        40  => with_composite(ext(Floor, WalkOnce, FK_LOWEST)),

        // ============ CEILINGS ============
        // LowerToFloor.
        41  => boom_fc(c,SwitchOnce, SPEED_SLOW, 0, CTO_F, 0, 0),
        43  => boom_fc(c,SwitchMany, SPEED_SLOW, 0, CTO_F, 0, 0),
        // Crushers (BOOM crusher: always down, crush, to floor+8).
        25  => boom_crusher(cr,WalkOnce,   SPEED_SLOW,   0), // CrushAndRaise (normal)
        49  => boom_crusher(cr,SwitchOnce, SPEED_SLOW,   0),
        73  => boom_crusher(cr,WalkMany,   SPEED_SLOW,   0),
        6   => boom_crusher(cr,WalkOnce,   SPEED_NORMAL, 0), // FastCrushAndRaise
        77  => boom_crusher(cr,WalkMany,   SPEED_NORMAL, 0),
        141 => boom_crusher(cr,WalkOnce,   SPEED_SLOW,   1), // SilentCrushAndRaise
        // lowerAndCrush: OG leaves crush=false; BOOM crusher forces crush -> extended.
        44  => ext(Ceiling, WalkOnce, CK_LOWER_CRUSH),
        72  => ext(Ceiling, WalkMany, CK_LOWER_CRUSH),

        // ============ DOORS (tagged, BOOM-exact) ============
        // Normal raise (open-delay-close, wait tier1). 4 is monster-openable.
        4   => boom_door(d,WalkOnce,   SPEED_NORMAL, DK_ODC, DELAY_T1) | DOOR_MONSTER_BIT,
        90  => boom_door(d,WalkMany,   SPEED_NORMAL, DK_ODC, DELAY_T1),
        29  => boom_door(d,SwitchOnce, SPEED_NORMAL, DK_ODC, DELAY_T1),
        63  => boom_door(d,SwitchMany, SPEED_NORMAL, DK_ODC, DELAY_T1),
        // Open and stay (no wait: never auto-closes, delay field irrelevant -> 0).
        2   => boom_door(d,WalkOnce,   SPEED_NORMAL, DK_O, 0),
        86  => boom_door(d,WalkMany,   SPEED_NORMAL, DK_O, 0),
        103 => boom_door(d,SwitchOnce, SPEED_NORMAL, DK_O, 0),
        61  => boom_door(d,SwitchMany, SPEED_NORMAL, DK_O, 0),
        46  => boom_door(d,GunMany,    SPEED_NORMAL, DK_O, 0),
        // Close and stay (no wait -> delay 0).
        3   => boom_door(d,WalkOnce,   SPEED_NORMAL, DK_C, 0),
        75  => boom_door(d,WalkMany,   SPEED_NORMAL, DK_C, 0),
        50  => boom_door(d,SwitchOnce, SPEED_NORMAL, DK_C, 0),
        42  => boom_door(d,SwitchMany, SPEED_NORMAL, DK_C, 0),
        // Close30ThenOpen (wait tier3 = 30s).
        16  => boom_door(d,WalkOnce,   SPEED_NORMAL, DK_CDO, DELAY_T3),
        76  => boom_door(d,WalkMany,   SPEED_NORMAL, DK_CDO, DELAY_T3),
        // Blaze raise.
        108 => boom_door(d,WalkOnce,   SPEED_FAST, DK_ODC, DELAY_T1),
        105 => boom_door(d,WalkMany,   SPEED_FAST, DK_ODC, DELAY_T1),
        111 => boom_door(d,SwitchOnce, SPEED_FAST, DK_ODC, DELAY_T1),
        114 => boom_door(d,SwitchMany, SPEED_FAST, DK_ODC, DELAY_T1),
        // Blaze open (no wait -> delay 0).
        109 => boom_door(d,WalkOnce,   SPEED_FAST, DK_O, 0),
        106 => boom_door(d,WalkMany,   SPEED_FAST, DK_O, 0),
        112 => boom_door(d,SwitchOnce, SPEED_FAST, DK_O, 0),
        115 => boom_door(d,SwitchMany, SPEED_FAST, DK_O, 0),
        // Blaze close (no wait -> delay 0).
        110 => boom_door(d,WalkOnce,   SPEED_FAST, DK_C, 0),
        107 => boom_door(d,WalkMany,   SPEED_FAST, DK_C, 0),
        113 => boom_door(d,SwitchOnce, SPEED_FAST, DK_C, 0),
        116 => boom_door(d,SwitchMany, SPEED_FAST, DK_C, 0),
        // ---- Manual doors (use/push, ev_vertical_door reads default_special) ----
        1   => with_manual(ext(Door, PushMany, DK_NORMAL)),
        31  => with_manual(ext(Door, PushMany, DK_OPEN)),
        117 => with_manual(ext(Door, PushMany, DK_BLAZE_RAISE)),
        118 => with_manual(ext(Door, PushMany, DK_BLAZE_OPEN)),
        26  => with_manual(ext(Door, PushMany, DK_NORMAL)),
        27  => with_manual(ext(Door, PushMany, DK_NORMAL)),
        28  => with_manual(ext(Door, PushMany, DK_NORMAL)),
        32  => with_manual(ext(Door, PushMany, DK_OPEN)),
        33  => with_manual(ext(Door, PushMany, DK_OPEN)),
        34  => with_manual(ext(Door, PushMany, DK_OPEN)),
        // Key switch-doors 99,133-137 are NOT normalized: they keep their
        // switch.rs arms (per-colour key check + PD_*O message). encode returns
        // None for them so the load pass leaves the vanilla number intact.

        // ============ LIFTS ============
        // DownWaitUpStay. 10/88 are monster-usable.
        10  => boom_lift(l,WalkOnce,   SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF) | LIFT_MONSTER_BIT,
        88  => boom_lift(l,WalkMany,   SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF) | LIFT_MONSTER_BIT,
        21  => boom_lift(l,SwitchOnce, SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF),
        62  => boom_lift(l,SwitchMany, SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF),
        // PerpetualRaise.
        53  => boom_lift(l,WalkOnce,   SPEED_NORMAL, LIFT_DELAY_T1, LNF_2_HNF),
        87  => boom_lift(l,WalkMany,   SPEED_NORMAL, LIFT_DELAY_T1, LNF_2_HNF),
        // BlazeDWUS.
        121 => boom_lift(l,WalkOnce,   SPEED_TURBO, LIFT_DELAY_T1, F2_LNF),
        120 => boom_lift(l,WalkMany,   SPEED_TURBO, LIFT_DELAY_T1, F2_LNF),
        122 => boom_lift(l,SwitchOnce, SPEED_TURBO, LIFT_DELAY_T1, F2_LNF),
        123 => boom_lift(l,SwitchMany, SPEED_TURBO, LIFT_DELAY_T1, F2_LNF),
        // ---- Lifts (extended: texture-change kinds, no BOOM lift change) ----
        22  => ext(Lift, WalkOnce,   PK_RAISE_NEAREST),
        95  => ext(Lift, WalkMany,   PK_RAISE_NEAREST),
        20  => ext(Lift, SwitchOnce, PK_RAISE_NEAREST),
        68  => ext(Lift, SwitchMany, PK_RAISE_NEAREST),
        47  => ext(Lift, GunOnce,    PK_RAISE_NEAREST),
        14  => with_amount(ext(Lift, SwitchOnce, PK_RAISE_CHANGE), 32),
        15  => with_amount(ext(Lift, SwitchOnce, PK_RAISE_CHANGE), 24),
        66  => with_amount(ext(Lift, SwitchMany, PK_RAISE_CHANGE), 24),
        67  => with_amount(ext(Lift, SwitchMany, PK_RAISE_CHANGE), 32),

        // ============ STAIRS ============
        8   => boom_stairs(s,WalkOnce,   SPEED_SLOW, STEP_8,  STAIR_UP),
        7   => boom_stairs(s,SwitchOnce, SPEED_SLOW, STEP_8,  STAIR_UP),
        100 => boom_stairs(s,WalkOnce,   SPEED_TURBO, STEP_16, STAIR_UP),
        127 => boom_stairs(s,SwitchOnce, SPEED_TURBO, STEP_16, STAIR_UP),

        _ => return None,
    };
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The complete vanilla mover set (every special the three OG dispatch
    /// tables route to a mover).
    const ALL_MOVERS: &[u32] = &[
        // floors
        5, 18, 19, 23, 24, 30, 36, 37, 38, 45, 55, 56, 59, 60, 64, 65, 69, 70, 71, 82, 83, 84, 91,
        92, 93, 94, 96, 98, 101, 102, 119, 128, 129, 130, 131, 132, 140, 9, 40, // ceilings
        6, 25, 41, 43, 44, 49, 72, 73, 77, 141,
        // doors (key switch-doors 99,133-137 keep their vanilla switch.rs arms)
        1, 2, 3, 4, 16, 26, 27, 28, 29, 31, 32, 33, 34, 42, 46, 50, 61, 63, 75, 76, 86, 90, 103,
        105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, // lifts
        10, 14, 15, 20, 21, 22, 47, 53, 62, 66, 67, 68, 87, 88, 95, 120, 121, 122, 123,
        // stairs
        7, 8, 100, 127,
    ];

    #[test]
    fn every_mover_encodes() {
        for &n in ALL_MOVERS {
            assert!(encode_vanilla(n).is_some(), "special {n} did not encode");
        }
    }

    #[test]
    fn idempotent_and_scoped() {
        assert_eq!(encode_vanilla(GEN_MIN), None);
        assert_eq!(encode_vanilla(0x4000), None);
        let e = encode_vanilla(5).unwrap();
        assert_eq!(
            encode_vanilla(e),
            None,
            "re-encoding generalized must be None"
        );
        for n in [
            0, 11, 12, 39, 51, 52, 54, 89, 97, 104, 124, 125, 126, 138, 139,
        ] {
            assert_eq!(encode_vanilla(n), None, "non-mover {n} must not encode");
        }
    }

    #[test]
    fn boom_specials_in_range_and_distinct() {
        let mut seen = std::collections::HashMap::new();
        for &n in ALL_MOVERS {
            let e = encode_vanilla(n).unwrap();
            if (GEN_MIN..=GEN_MAX).contains(&e) {
                // Distinct BOOM specials per distinct behaviour. Manual/key
                // doors are extended; remaining BOOM specials must be unique.
                if let Some(&prev) = seen.get(&e) {
                    panic!("specials {prev} and {n} both encode to BOOM {e:#x}");
                }
                seen.insert(e, n);
            } else {
                assert!(
                    is_extended(e),
                    "special {n} -> {e:#x} neither BOOM nor extended"
                );
            }
        }
    }

    #[test]
    fn extended_flags_decode() {
        assert!(decode_ext(encode_vanilla(40).unwrap()).unwrap().composite);
        assert!(decode_ext(encode_vanilla(9).unwrap()).unwrap().composite == false);
        assert!(decode_ext(encode_vanilla(1).unwrap()).unwrap().manual);
        assert_eq!(decode_ext(encode_vanilla(14).unwrap()).unwrap().amount, 32);
        assert_eq!(decode_ext(encode_vanilla(15).unwrap()).unwrap().amount, 24);
    }

    /// Every vanilla mover must decode (via encode -> decode) back to the
    /// exact room4doom (category, kind) its OG dispatch produces. This is the
    /// demo-safety round-trip: the engine, after load-rewrite, calls
    /// ev_do_*(kind) with this kind.
    #[test]
    fn decode_round_trip_kinds() {
        use Category::*;
        // (special, category, room4doom kind discriminant)
        let cases: &[(u32, Category, u8)] = &[
            // floors
            (19, Floor, 0),
            (83, Floor, 0),
            (102, Floor, 0),
            (45, Floor, 0),
            (38, Floor, 1),
            (82, Floor, 1),
            (23, Floor, 1),
            (60, Floor, 1),
            (5, Floor, 3),
            (91, Floor, 3),
            (101, Floor, 3),
            (64, Floor, 3),
            (24, Floor, 3),
            (30, Floor, 5),
            (96, Floor, 5),
            (92, Floor, 7),
            (59, Floor, 8),
            (93, Floor, 8),
            (37, Floor, 6),
            (84, Floor, 6),
            (36, Floor, 2),
            (98, Floor, 2),
            (71, Floor, 2),
            (70, Floor, 2),
            (56, Floor, 9),
            (94, Floor, 9),
            (55, Floor, 9),
            (65, Floor, 9),
            (119, Floor, 4),
            (128, Floor, 4),
            (18, Floor, 4),
            (69, Floor, 4),
            (130, Floor, 10),
            (129, Floor, 10),
            (131, Floor, 10),
            (132, Floor, 10),
            // ceilings
            (41, Ceiling, 0),
            (43, Ceiling, 0),
            (25, Ceiling, 3),
            (49, Ceiling, 3),
            (73, Ceiling, 3),
            (6, Ceiling, 4),
            (77, Ceiling, 4),
            (141, Ceiling, 5),
            // doors (BOOM-exact only; manual/key are extended)
            (4, Door, 0),
            (90, Door, 0),
            (29, Door, 0),
            (63, Door, 0),
            (2, Door, 3),
            (86, Door, 3),
            (103, Door, 3),
            (61, Door, 3),
            (46, Door, 3),
            (3, Door, 2),
            (75, Door, 2),
            (50, Door, 2),
            (42, Door, 2),
            (108, Door, 5),
            (105, Door, 5),
            (111, Door, 5),
            (114, Door, 5),
            (109, Door, 6),
            (106, Door, 6),
            (112, Door, 6),
            (115, Door, 6),
            (110, Door, 7),
            (107, Door, 7),
            (113, Door, 7),
            (116, Door, 7),
            // lifts
            (10, Lift, 1),
            (88, Lift, 1),
            (21, Lift, 1),
            (62, Lift, 1),
            (53, Lift, 0),
            (87, Lift, 0),
            (121, Lift, 4),
            (120, Lift, 4),
            (122, Lift, 4),
            (123, Lift, 4),
            // stairs
            (8, Stairs, 0),
            (7, Stairs, 0),
            (100, Stairs, 1),
            (127, Stairs, 1),
        ];
        for &(n, cat, kind) in cases {
            let e = encode_vanilla(n).unwrap();
            let spec = decode(e).unwrap_or_else(|| panic!("special {n} -> {e:#x} did not decode"));
            assert_eq!(spec.category, cat, "special {n} category");
            assert_eq!(spec.kind, kind, "special {n} kind (got {})", spec.kind);
        }
    }

    /// Extended movers decode to the right category + kind + flags too.
    #[test]
    fn decode_round_trip_extended() {
        use Category::*;
        let cases: &[(u32, Category, u8)] = &[
            (140, Floor, 12), // RaiseFloor512
            (9, Floor, 11),   // DonutRaise
            (44, Ceiling, 2), // LowerAndCrush
            (72, Ceiling, 2),
            (1, Door, 0),  // manual Normal
            (31, Door, 3), // manual Open
            (22, Lift, 3), // RaiseToNearestAndChange
            (14, Lift, 2), // RaiseAndChange
        ];
        for &(n, cat, kind) in cases {
            let spec = decode(encode_vanilla(n).unwrap()).unwrap();
            assert_eq!(spec.category, cat, "special {n} category");
            assert_eq!(spec.kind, kind, "special {n} kind");
        }
    }

    /// Audit-verified BOOM hex values (recomputed by the workflow critic from
    /// BOOM P_SPEC.H). Asserting against them proves our field builders pack
    /// bits identically to the authoritative layout.
    #[test]
    fn boom_hex_matches_audit() {
        let cases: &[(u32, u32)] = &[
            // floors
            (5, 0x6240),
            (19, 0x6080),
            (30, 0x62C0),
            (36, 0x6090),
            (37, 0x6900),
            (38, 0x6100),
            (56, 0x7240),
            (59, 0x6B40),
            (82, 0x6101),
            (83, 0x6081),
            (84, 0x6901),
            (91, 0x6241),
            (92, 0x6341),
            (93, 0x6B41),
            (94, 0x7241),
            (96, 0x62C1),
            (98, 0x6091),
            (119, 0x61C0),
            (128, 0x61C1),
            (129, 0x61D1),
            (130, 0x61D0),
            (24, 0x6244),
            // doors
            // 4 carries the BOOM door monster bit (bit 7) -> 0x3D08 | 0x80.
            (2, 0x3C28),
            (3, 0x3C68),
            (4, 0x3D88),
            (16, 0x3F48),
            (29, 0x3D0A),
            // 46 is gun-REPEATABLE per OG (P_ChangeSwitchTexture arg 1); the
            // audit's recomputed 0x3C2C used GunOnce in error. GunMany = 0x3C2D.
            (42, 0x3C6B),
            (46, 0x3C2D),
            (50, 0x3C6A),
            (61, 0x3C2B),
            (63, 0x3D0B),
            (75, 0x3C69),
            (76, 0x3F49),
            (86, 0x3C29),
            (90, 0x3D09),
            (103, 0x3C2A),
            (105, 0x3D11),
            (106, 0x3C31),
            (107, 0x3C71),
            (108, 0x3D10),
            (109, 0x3C30),
            (110, 0x3C70),
            (111, 0x3D12),
            (112, 0x3C32),
            (113, 0x3C72),
            (114, 0x3D13),
            (115, 0x3C33),
            (116, 0x3C73),
            // lifts (10/88 carry the BOOM lift monster bit (bit 5) -> | 0x20)
            (10, 0x3468),
            (53, 0x3748),
            (87, 0x3749),
            (88, 0x3469),
            (120, 0x3459),
            (121, 0x3458),
            (21, 0x344A),
            (62, 0x344B),
            (122, 0x345A),
            (123, 0x345B),
        ];
        for &(n, want) in cases {
            assert_eq!(encode_vanilla(n), Some(want), "special {n} hex");
        }
    }
}
