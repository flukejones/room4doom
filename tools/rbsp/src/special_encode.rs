//! Canonical mover encoding: at level load, rewrite every vanilla (Doom 1-141)
//! mover linedef special into a generalized form so the engine has one decode
//! path for movers.
//!
//! Two target forms, both decoded by the same generalized handler:
//!
//! - **Standard BOOM** (`0x2F80..=0x7FFF`): when a vanilla special's behaviour
//!   is exactly expressible in BOOM's generalized bitfields (speed/model/
//!   target/direction/change/crush/delay/key), it is rewritten to the real BOOM
//!   special. Hand-authored BOOM maps already use this range and decode
//!   identically. Each value is built from the field constants below, never
//!   hand-typed, so the bit packing is correct by construction.
//! - **Extended** (`>= EXT_BASE`, the otherwise-unused high `u32` range): for
//!   kinds BOOM cannot name exactly — donut, the composite vanilla-40, manual
//!   doors (ev_vertical_door), RaiseFloor512, the platform texture-change
//!   kinds, and `lowerAndCrush` (OG leaves crush off, which BOOM's crusher
//!   cannot express). The extended form carries the exact mover identity in
//!   explicit fields.
//!
//! This module is also the home of the mover kind vocabulary itself: the
//! [`FloorKind`]/[`CeilKind`]/[`DoorKind`]/[`PlatKind`]/[`StairKind`] enums
//! whose discriminants are the encoding's `kind` field, wrapped per category
//! into [`MoverKind`] by [`decode`].
//!
//! Behaviour is verified against OG Doom C (the demo-safety oracle) cross-
//! checked with BOOM 2.02 P_GENLIN.C / P_SPEC.H. `encode_vanilla` is idempotent
//! and only rewrites vanilla numbers in `1..=141`.

use self::Trigger::{
    GunMany, GunOnce, PushMany, PushOnce, SwitchMany, SwitchOnce, WalkMany, WalkOnce,
};

/// Declare a `#[repr(u8)]` enum and its `TryFrom<u8>` from one variant list,
/// so the two cannot drift.
#[macro_export]
macro_rules! byte_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident { $($(#[$vmeta:meta])* $variant:ident = $value:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name { $($(#[$vmeta])* $variant = $value),+ }

        impl TryFrom<u8> for $name {
            /// The raw byte that failed to map to a variant.
            type Error = u8;

            fn try_from(v: u8) -> Result<Self, u8> {
                match v {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(v),
                }
            }
        }
    };
}

byte_enum! {
    /// Floor mover kinds (`p_floor`).
    pub enum FloorKind {
        /// lower floor to highest surrounding floor
        LowerFloor = 0,
        /// lower floor to lowest surrounding floor
        LowerFloorToLowest = 1,
        /// lower floor to highest surrounding floor VERY FAST
        TurboLower = 2,
        /// raise floor to lowest surrounding CEILING
        RaiseFloor = 3,
        /// raise floor to next highest surrounding floor
        RaiseFloorToNearest = 4,
        /// raise floor to shortest height with same texture around it
        RaiseToTexture = 5,
        /// lower floor to lowest surrounding floor and change floorpic
        LowerAndChange = 6,
        /// Raise floor 24 units from start
        RaiseFloor24 = 7,
        /// Raise floor 24 units from start and change texture
        RaiseFloor24andChange = 8,
        /// Raise floor and crush all entities on it
        RaiseFloorCrush = 9,
        /// raise to next highest floor, turbo-speed
        RaiseFloorTurbo = 10,
        /// Do donuts
        DonutRaise = 11,
        /// Raise floor 512 units from start
        RaiseFloor512 = 12,
    }
}

byte_enum! {
    /// Stair build kinds (`p_floor` EV_BuildStairs).
    pub enum StairKind {
        /// slowly build by 8
        Build8 = 0,
        /// quickly build by 16
        Turbo16 = 1,
    }
}

byte_enum! {
    /// Ceiling mover kinds (`p_ceilng`).
    pub enum CeilKind {
        LowerToFloor = 0,
        RaiseToHighest = 1,
        LowerAndCrush = 2,
        CrushAndRaise = 3,
        FastCrushAndRaise = 4,
        SilentCrushAndRaise = 5,
    }
}

byte_enum! {
    /// Door mover kinds (`p_doors`).
    pub enum DoorKind {
        Normal = 0,
        Close30ThenOpen = 1,
        Close = 2,
        Open = 3,
        RaiseIn5Mins = 4,
        BlazeRaise = 5,
        BlazeOpen = 6,
        BlazeClose = 7,
    }
}

byte_enum! {
    /// Platform / lift mover kinds (`p_plats`).
    pub enum PlatKind {
        PerpetualRaise = 0,
        DownWaitUpStay = 1,
        RaiseAndChange = 2,
        RaiseToNearestAndChange = 3,
        BlazeDWUS = 4,
    }
}

byte_enum! {
    /// BOOM generalized lock kinds (P_SPEC.H locked-door key field order).
    pub enum LockKind {
        AnyKey = 0,
        RedCard = 1,
        BlueCard = 2,
        YellowCard = 3,
        RedSkull = 4,
        BlueSkull = 5,
        YellowSkull = 6,
        AllKeys = 7,
    }
}

/// Key requirement carried by a locked door special.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorLock {
    pub kind: LockKind,
    /// BOOM "skulls are keys": a skull satisfies its card colour and vice
    /// versa. All vanilla locked doors behave this way.
    pub skulls_are_cards: bool,
}

/// Lowest standard BOOM generalized special.
pub const GEN_MIN: u32 = 0x2F80;
/// Highest standard BOOM generalized special (16-bit positive max).
pub const GEN_MAX: u32 = 0x7FFF;
/// Base of the engine-extended namespace: above all 16-bit specials AND above
/// every extended payload bit (payload occupies bits 0-19).
pub const EXT_BASE: u32 = 0x0100_0000;

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
const LOCKED_SPEED_SHIFT: u32 = 3;
const LOCKED_KIND_SHIFT: u32 = 5;
const LOCKED_KEY_SHIFT: u32 = 6;
const LOCKED_SK_SHIFT: u32 = 9;
const LIFT_SPEED_SHIFT: u32 = 3;
const LIFT_DELAY_SHIFT: u32 = 6;
const LIFT_TARGET_SHIFT: u32 = 8;
const STAIR_SPEED_SHIFT: u32 = 3;
const STAIR_STEP_SHIFT: u32 = 6;
const STAIR_DIR_SHIFT: u32 = 8;
const CRUSHER_SPEED_SHIFT: u32 = 3;
const CRUSHER_SILENT_SHIFT: u32 = 6;

// BOOM monster-activation bits.
const DOOR_MONSTER_BIT: u32 = 1 << 7; // P_SPEC.H DoorMonster = 0x80
const FC_MONSTER_BIT: u32 = 1 << 5; // floor/ceiling/lift/stairs/crusher = 0x20

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
            Self::WalkMany | Self::SwitchMany | Self::GunMany | Self::PushMany
        )
    }
}

/// Mover category: which mover family the special drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Floor,
    Ceiling,
    Door,
    Lift,
    Stairs,
}

/// The exact mover a special drives: category and kind in one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoverKind {
    Floor(FloorKind),
    Ceiling(CeilKind),
    Door(DoorKind),
    Lift(PlatKind),
    Stairs(StairKind),
}

impl MoverKind {
    pub const fn category(self) -> Category {
        match self {
            Self::Floor(_) => Category::Floor,
            Self::Ceiling(_) => Category::Ceiling,
            Self::Door(_) => Category::Door,
            Self::Lift(_) => Category::Lift,
            Self::Stairs(_) => Category::Stairs,
        }
    }

    const fn kind_byte(self) -> u8 {
        match self {
            Self::Floor(k) => k as u8,
            Self::Ceiling(k) => k as u8,
            Self::Door(k) => k as u8,
            Self::Lift(k) => k as u8,
            Self::Stairs(k) => k as u8,
        }
    }
}

// --- Extended namespace layout (bits above EXT_BASE) --------------------
// bits 0-2 category, 3-5 trigger, 6-9 kind, 10-11 amount (0/24/32),
// 12 manual, 13 composite, 14-17 lock (0 none, else LockKind + 1),
// 18 skulls-are-cards, 19 monster-activatable.
const EXT_AMOUNT_SHIFT: u32 = 10;
const EXT_MANUAL_BIT: u32 = 1 << 12;
const EXT_COMPOSITE_BIT: u32 = 1 << 13;
const EXT_LOCK_SHIFT: u32 = 14;
const EXT_SAC_BIT: u32 = 1 << 18;
const EXT_MONSTER_BIT: u32 = 1 << 19;

impl Category {
    const fn ext_bits(self) -> u32 {
        self as u32
    }
    fn from_ext_bits(v: u32) -> Option<Self> {
        match v & 0x7 {
            0 => Some(Self::Floor),
            1 => Some(Self::Ceiling),
            2 => Some(Self::Door),
            3 => Some(Self::Lift),
            4 => Some(Self::Stairs),
            _ => None,
        }
    }
}

fn trigger_from_bits(v: u32) -> Trigger {
    match v & 0x7 {
        0 => WalkOnce,
        1 => WalkMany,
        2 => SwitchOnce,
        3 => SwitchMany,
        4 => GunOnce,
        5 => GunMany,
        6 => PushOnce,
        _ => PushMany,
    }
}

/// Decoded identity of a generalized mover special.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedSpecial {
    pub trigger: Trigger,
    pub kind: MoverKind,
    /// Platform RaiseAndChange height (0, 24 or 32).
    pub amount: i32,
    /// Door routes through `ev_vertical_door`.
    pub manual: bool,
    /// Floor also fires ceiling RaiseToHighest + floor LowerFloorToLowest.
    pub composite: bool,
    /// Key requirement; activation fails without the key.
    pub lock: Option<DoorLock>,
    /// Monsters may activate this special.
    pub monster: bool,
}

/// True if `special` is an engine-extended mover special.
pub fn is_extended(special: u32) -> bool {
    special >= EXT_BASE
}

/// True if `special` is any generalized mover (standard BOOM or extended).
pub fn is_generalized(special: u32) -> bool {
    (GEN_MIN..=GEN_MAX).contains(&special) || special >= EXT_BASE
}

const fn ext(trig: Trigger, kind: MoverKind) -> u32 {
    EXT_BASE | kind.category().ext_bits() | (trig.bits() << 3) | ((kind.kind_byte() as u32) << 6)
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
const fn with_lock(s: u32, kind: LockKind) -> u32 {
    // Vanilla locked doors all accept card or skull.
    s | ((kind as u32 + 1) << EXT_LOCK_SHIFT) | EXT_SAC_BIT
}
const fn with_monster(s: u32) -> u32 {
    s | EXT_MONSTER_BIT
}

/// Decode an extended special. `None` for standard BOOM and vanilla specials.
fn decode_ext(special: u32) -> Option<DecodedSpecial> {
    if !is_extended(special) {
        return None;
    }
    let v = special - EXT_BASE;
    let category = Category::from_ext_bits(v)?;
    let trigger = trigger_from_bits(v >> 3);
    let kind_byte = ((v >> 6) & 0xF) as u8;
    let kind = match category {
        Category::Floor => MoverKind::Floor(FloorKind::try_from(kind_byte).ok()?),
        Category::Ceiling => MoverKind::Ceiling(CeilKind::try_from(kind_byte).ok()?),
        Category::Door => MoverKind::Door(DoorKind::try_from(kind_byte).ok()?),
        Category::Lift => MoverKind::Lift(PlatKind::try_from(kind_byte).ok()?),
        Category::Stairs => MoverKind::Stairs(StairKind::try_from(kind_byte).ok()?),
    };
    let amount = match (v >> EXT_AMOUNT_SHIFT) & 0x3 {
        1 => 24,
        2 => 32,
        _ => 0,
    };
    let lock_bits = (v >> EXT_LOCK_SHIFT) & 0xF;
    let lock = if lock_bits == 0 {
        None
    } else {
        Some(DoorLock {
            kind: LockKind::try_from(u8::try_from(lock_bits - 1).ok()?).ok()?,
            skulls_are_cards: v & EXT_SAC_BIT != 0,
        })
    };
    Some(DecodedSpecial {
        trigger,
        kind,
        amount,
        manual: (v & EXT_MANUAL_BIT) != 0,
        composite: (v & EXT_COMPOSITE_BIT) != 0,
        lock,
        monster: (v & EXT_MONSTER_BIT) != 0,
    })
}

/// Resolve a standard BOOM special to its category by base address.
fn boom_category(special: u32) -> Option<Category> {
    if special >= GEN_FLOOR_BASE {
        Some(Category::Floor)
    } else if special >= GEN_CEILING_BASE {
        Some(Category::Ceiling)
    } else if special >= GEN_LOCKED_BASE {
        // Regular doors and locked doors (lower base) both decode as Door; the
        // decoder picks the layout/base by range.
        Some(Category::Door)
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
pub fn decode(special: u32) -> Option<DecodedSpecial> {
    if is_extended(special) {
        return decode_ext(special);
    }
    if !(GEN_MIN..=GEN_MAX).contains(&special) {
        return None;
    }
    let category = boom_category(special)?;
    let trigger = trigger_from_bits(special >> TRIGGER_SHIFT);
    let (kind, lock, monster) = match category {
        Category::Floor => {
            let v = special - GEN_FLOOR_BASE;
            (
                MoverKind::Floor(decode_floor_kind(v)),
                None,
                v & FC_MONSTER_BIT != 0,
            )
        }
        Category::Ceiling => {
            // Crusher (0x2F80) and ceiling (0x4000) share the monster bit.
            let base = if special >= GEN_CEILING_BASE {
                GEN_CEILING_BASE
            } else {
                GEN_CRUSHER_BASE
            };
            (
                MoverKind::Ceiling(decode_ceiling_kind(special)),
                None,
                (special - base) & FC_MONSTER_BIT != 0,
            )
        }
        // Locked doors (`[GEN_LOCKED_BASE, GEN_DOOR_BASE)`) share the Door
        // category but use a distinct bitfield layout and base.
        Category::Door if special >= GEN_DOOR_BASE => {
            let v = special - GEN_DOOR_BASE;
            (
                MoverKind::Door(decode_door_kind(v)),
                None,
                v & DOOR_MONSTER_BIT != 0,
            )
        }
        Category::Door => {
            let v = special - GEN_LOCKED_BASE;
            let key = ((v >> LOCKED_KEY_SHIFT) & 0x7) as u8;
            let lock = DoorLock {
                kind: LockKind::try_from(key).expect("3-bit key field"),
                skulls_are_cards: (v >> LOCKED_SK_SHIFT) & 0x1 != 0,
            };
            (
                MoverKind::Door(decode_locked_door_kind(v)),
                Some(lock),
                false,
            )
        }
        Category::Lift => {
            let v = special - GEN_LIFT_BASE;
            (
                MoverKind::Lift(decode_lift_kind(v)),
                None,
                v & FC_MONSTER_BIT != 0,
            )
        }
        Category::Stairs => {
            let v = special - GEN_STAIRS_BASE;
            (
                MoverKind::Stairs(decode_stair_kind(v)),
                None,
                v & FC_MONSTER_BIT != 0,
            )
        }
    };
    Some(DecodedSpecial {
        trigger,
        kind,
        amount: 0,
        manual: false,
        composite: false,
        lock,
        monster,
    })
}

fn field(v: u32, shift: u32, mask: u32) -> u32 {
    (v >> shift) & mask
}

fn decode_floor_kind(v: u32) -> FloorKind {
    let speed = field(v, FC_SPEED_SHIFT, 0x3);
    let dir = field(v, FC_DIR_SHIFT, 0x1);
    let target = field(v, FC_TARGET_SHIFT, 0x7);
    let change = field(v, FC_CHANGE_SHIFT, 0x3);
    let crush = field(v, FC_CRUSH_SHIFT, 0x1);
    let fast = speed >= SPEED_FAST;
    if dir == 0 {
        // down
        #[allow(
            clippy::match_same_arms,
            reason = "explicit bit-field decode table; named arms document the mapping"
        )]
        match (target, change) {
            (FTO_LNF, CHG_TXT) => FloorKind::LowerAndChange,
            (FTO_LNF, _) => FloorKind::LowerFloorToLowest,
            (FTO_HNF, _) if fast => FloorKind::TurboLower,
            (FTO_HNF, _) => FloorKind::LowerFloor,
            _ => FloorKind::LowerFloor,
        }
    } else {
        // up
        if crush == 1 {
            return FloorKind::RaiseFloorCrush;
        }
        #[allow(
            clippy::match_same_arms,
            reason = "explicit bit-field decode table; named arms document the mapping"
        )]
        match (target, change, fast) {
            (FTO_LNC, ..) => FloorKind::RaiseFloor,
            (FBY_ST, ..) => FloorKind::RaiseToTexture,
            (FBY_24, CHG_TXT, _) => FloorKind::RaiseFloor24andChange,
            (FBY_24, ..) => FloorKind::RaiseFloor24,
            (FTO_NNF, _, true) => FloorKind::RaiseFloorTurbo,
            (FTO_NNF, _, false) => FloorKind::RaiseFloorToNearest,
            _ => FloorKind::RaiseFloor,
        }
    }
}

fn decode_ceiling_kind(special: u32) -> CeilKind {
    if special >= GEN_CEILING_BASE {
        // GenCeiling: only LowerToFloor used (CtoF, down).
        CeilKind::LowerToFloor
    } else {
        // GenCrusher.
        let v = special - GEN_CRUSHER_BASE;
        let speed = field(v, CRUSHER_SPEED_SHIFT, 0x3);
        let silent = field(v, CRUSHER_SILENT_SHIFT, 0x1);
        if silent == 1 {
            CeilKind::SilentCrushAndRaise
        } else if speed >= SPEED_NORMAL {
            CeilKind::FastCrushAndRaise
        } else {
            CeilKind::CrushAndRaise
        }
    }
}

fn decode_door_kind(v: u32) -> DoorKind {
    let speed = field(v, DOOR_SPEED_SHIFT, 0x3);
    let kind = field(v, DOOR_KIND_SHIFT, 0x3);
    let fast = speed >= SPEED_FAST;
    #[allow(
        clippy::match_same_arms,
        reason = "explicit bit-field decode table; named arms document the mapping"
    )]
    match (kind, fast) {
        (DK_ODC, false) => DoorKind::Normal,
        (DK_ODC, true) => DoorKind::BlazeRaise,
        (DK_O, false) => DoorKind::Open,
        (DK_O, true) => DoorKind::BlazeOpen,
        (DK_C, false) => DoorKind::Close,
        (DK_C, true) => DoorKind::BlazeClose,
        // DK_CDO (close30ThenOpen) is encoded extended, never reaches here.
        _ => DoorKind::Normal,
    }
}

/// Generalized locked door (`GEN_LOCKED_BASE`). Same speed field as a regular
/// door but `kind` is a single bit (0 = open-delay-close/raise, 1 = open-stay);
/// locked doors never close-only.
fn decode_locked_door_kind(v: u32) -> DoorKind {
    let speed = field(v, LOCKED_SPEED_SHIFT, 0x3);
    let open = field(v, LOCKED_KIND_SHIFT, 0x1) == 1;
    let fast = speed >= SPEED_FAST;
    match (open, fast) {
        (false, false) => DoorKind::Normal,
        (false, true) => DoorKind::BlazeRaise,
        (true, false) => DoorKind::Open,
        (true, true) => DoorKind::BlazeOpen,
    }
}

fn decode_lift_kind(v: u32) -> PlatKind {
    let speed = field(v, LIFT_SPEED_SHIFT, 0x3);
    let target = field(v, LIFT_TARGET_SHIFT, 0x3);
    if target == LNF_2_HNF {
        PlatKind::PerpetualRaise
    } else if speed >= SPEED_TURBO {
        PlatKind::BlazeDWUS
    } else {
        PlatKind::DownWaitUpStay
    }
}

fn decode_stair_kind(v: u32) -> StairKind {
    let speed = field(v, STAIR_SPEED_SHIFT, 0x3);
    if speed >= SPEED_FAST {
        StairKind::Turbo16
    } else {
        StairKind::Build8
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

const fn boom_door(base: u32, trig: Trigger, speed: u32, kind: u32, delay: u32) -> u32 {
    base | trig.bits()
        | (speed << DOOR_SPEED_SHIFT)
        | (kind << DOOR_KIND_SHIFT)
        | (delay << DOOR_DELAY_SHIFT)
}
const fn boom_locked(trig: Trigger, speed: u32, open_stay: bool, key: LockKind) -> u32 {
    // Vanilla key switch-doors accept card or skull -> skulls-are-keys set.
    GEN_LOCKED_BASE
        | trig.bits()
        | (speed << LOCKED_SPEED_SHIFT)
        | ((open_stay as u32) << LOCKED_KIND_SHIFT)
        | ((key as u32) << LOCKED_KEY_SHIFT)
        | (1 << LOCKED_SK_SHIFT)
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
    let f = GEN_FLOOR_BASE;
    let c = GEN_CEILING_BASE;
    let d = GEN_DOOR_BASE;
    let l = GEN_LIFT_BASE;
    let s = GEN_STAIRS_BASE;
    let cr = GEN_CRUSHER_BASE;

    #[allow(
        clippy::match_same_arms,
        reason = "one arm per vanilla line-special number; distinct keys may share an encoding"
    )]
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
        140 => ext(SwitchOnce, MoverKind::Floor(FloorKind::RaiseFloor512)),
        9   => ext(SwitchOnce, MoverKind::Floor(FloorKind::DonutRaise)),
        40  => with_composite(ext(WalkOnce, MoverKind::Floor(FloorKind::LowerFloorToLowest))),

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
        44  => ext(WalkOnce, MoverKind::Ceiling(CeilKind::LowerAndCrush)),
        72  => ext(WalkMany, MoverKind::Ceiling(CeilKind::LowerAndCrush)),

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
        // 46 is monster-shootable in OG P_ShootSpecialLine.
        46  => boom_door(d,GunMany,    SPEED_NORMAL, DK_O, 0) | DOOR_MONSTER_BIT,
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
        // ---- Key switch-doors (BOOM locked range, key check at dispatch) ----
        99  => boom_locked(SwitchMany, SPEED_FAST, true, LockKind::BlueCard),
        133 => boom_locked(SwitchOnce, SPEED_FAST, true, LockKind::BlueCard),
        134 => boom_locked(SwitchMany, SPEED_FAST, true, LockKind::RedCard),
        135 => boom_locked(SwitchOnce, SPEED_FAST, true, LockKind::RedCard),
        136 => boom_locked(SwitchMany, SPEED_FAST, true, LockKind::YellowCard),
        137 => boom_locked(SwitchOnce, SPEED_FAST, true, LockKind::YellowCard),
        // ---- Manual doors (push, ev_vertical_door) ----
        // 1/32/33/34 are the OG monster-usable set; 26-28/32-34 carry locks.
        1   => with_monster(with_manual(ext(PushMany, MoverKind::Door(DoorKind::Normal)))),
        31  => with_manual(ext(PushOnce, MoverKind::Door(DoorKind::Open))),
        117 => with_manual(ext(PushMany, MoverKind::Door(DoorKind::BlazeRaise))),
        118 => with_manual(ext(PushOnce, MoverKind::Door(DoorKind::BlazeOpen))),
        26  => with_lock(with_manual(ext(PushMany, MoverKind::Door(DoorKind::Normal))),
                         LockKind::BlueCard),
        27  => with_lock(with_manual(ext(PushMany, MoverKind::Door(DoorKind::Normal))),
                         LockKind::YellowCard),
        28  => with_lock(with_manual(ext(PushMany, MoverKind::Door(DoorKind::Normal))),
                         LockKind::RedCard),
        32  => with_monster(with_lock(with_manual(ext(PushOnce, MoverKind::Door(DoorKind::Open))),
                                      LockKind::BlueCard)),
        33  => with_monster(with_lock(with_manual(ext(PushOnce, MoverKind::Door(DoorKind::Open))),
                                      LockKind::RedCard)),
        34  => with_monster(with_lock(with_manual(ext(PushOnce, MoverKind::Door(DoorKind::Open))),
                                      LockKind::YellowCard)),

        // ============ LIFTS ============
        // DownWaitUpStay. 10/88 are monster-usable.
        10  => boom_lift(l,WalkOnce,   SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF) | FC_MONSTER_BIT,
        88  => boom_lift(l,WalkMany,   SPEED_NORMAL, LIFT_DELAY_T1, F2_LNF) | FC_MONSTER_BIT,
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
        22  => ext(WalkOnce,   MoverKind::Lift(PlatKind::RaiseToNearestAndChange)),
        95  => ext(WalkMany,   MoverKind::Lift(PlatKind::RaiseToNearestAndChange)),
        20  => ext(SwitchOnce, MoverKind::Lift(PlatKind::RaiseToNearestAndChange)),
        68  => ext(SwitchMany, MoverKind::Lift(PlatKind::RaiseToNearestAndChange)),
        47  => ext(GunOnce,    MoverKind::Lift(PlatKind::RaiseToNearestAndChange)),
        14  => with_amount(ext(SwitchOnce, MoverKind::Lift(PlatKind::RaiseAndChange)), 32),
        15  => with_amount(ext(SwitchOnce, MoverKind::Lift(PlatKind::RaiseAndChange)), 24),
        66  => with_amount(ext(SwitchMany, MoverKind::Lift(PlatKind::RaiseAndChange)), 24),
        67  => with_amount(ext(SwitchMany, MoverKind::Lift(PlatKind::RaiseAndChange)), 32),

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
    use std::collections::HashMap;

    use super::CeilKind::*;
    use super::DoorKind::*;
    use super::FloorKind::*;
    use super::MoverKind::{Ceiling as C, Door as D, Floor as F, Lift as L, Stairs as S};
    use super::PlatKind::*;
    use super::StairKind::*;
    use super::*;

    /// The complete vanilla mover set (every special the three OG dispatch
    /// tables route to a mover).
    const ALL_MOVERS: &[u32] = &[
        // floors
        5, 18, 19, 23, 24, 30, 36, 37, 38, 45, 55, 56, 59, 60, 64, 65, 69, 70, 71, 82, 83, 84, 91,
        92, 93, 94, 96, 98, 101, 102, 119, 128, 129, 130, 131, 132, 140, 9, 40, // ceilings
        6, 25, 41, 43, 44, 49, 72, 73, 77, 141, // doors
        1, 2, 3, 4, 16, 26, 27, 28, 29, 31, 32, 33, 34, 42, 46, 50, 61, 63, 75, 76, 86, 90, 99,
        103, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 133, 134, 135,
        136, 137, // lifts
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
        let mut seen = HashMap::new();
        for &n in ALL_MOVERS {
            let e = encode_vanilla(n).unwrap();
            if (GEN_MIN..=GEN_MAX).contains(&e) {
                // Distinct BOOM specials per distinct behaviour. Manual doors
                // are extended; remaining BOOM specials must be unique.
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
        let d = |n: u32| decode(encode_vanilla(n).unwrap()).unwrap();
        assert!(d(40).composite);
        assert!(!d(9).composite);
        assert!(d(1).manual);
        assert_eq!(d(14).amount, 32);
        assert_eq!(d(15).amount, 24);
    }

    /// Lock and monster flags survive the encode -> decode round trip with the
    /// OG semantics: 1/32/33/34 monster-usable, 4/10/46/88 monster-triggered,
    /// vanilla locks accept card or skull.
    #[test]
    fn lock_and_monster_decode() {
        let d = |n: u32| decode(encode_vanilla(n).unwrap()).unwrap();
        let lock = |k| {
            Some(DoorLock {
                kind: k,
                skulls_are_cards: true,
            })
        };
        assert_eq!(d(26).lock, lock(LockKind::BlueCard));
        assert_eq!(d(27).lock, lock(LockKind::YellowCard));
        assert_eq!(d(28).lock, lock(LockKind::RedCard));
        assert_eq!(d(33).lock, lock(LockKind::RedCard));
        assert_eq!(d(99).lock, lock(LockKind::BlueCard));
        assert_eq!(d(137).lock, lock(LockKind::YellowCard));
        assert_eq!(d(1).lock, None);
        assert_eq!(d(2).lock, None);

        for n in [1, 4, 10, 32, 33, 34, 46, 88] {
            assert!(d(n).monster, "special {n} must allow monsters");
        }
        for n in [2, 26, 29, 31, 62, 99, 117, 118] {
            assert!(!d(n).monster, "special {n} must not allow monsters");
        }
    }

    /// Once/many trigger classes for manual and key doors match OG dispatch:
    /// zeroed-after-use doors are PushOnce/SwitchOnce.
    #[test]
    fn door_triggers_match_og() {
        let t = |n: u32| decode(encode_vanilla(n).unwrap()).unwrap().trigger;
        for n in [1, 26, 27, 28, 117] {
            assert_eq!(t(n), PushMany, "special {n}");
        }
        for n in [31, 32, 33, 34, 118] {
            assert_eq!(t(n), PushOnce, "special {n}");
        }
        for n in [99, 134, 136] {
            assert_eq!(t(n), SwitchMany, "special {n}");
        }
        for n in [133, 135, 137] {
            assert_eq!(t(n), SwitchOnce, "special {n}");
        }
    }

    /// Every vanilla mover must decode (via encode -> decode) back to the
    /// exact room4doom mover kind its OG dispatch produces. This is the
    /// demo-safety round-trip: the engine, after load-rewrite, dispatches on
    /// this kind.
    #[test]
    fn decode_round_trip_kinds() {
        let cases: &[(u32, MoverKind)] = &[
            // floors
            (19, F(LowerFloor)),
            (83, F(LowerFloor)),
            (102, F(LowerFloor)),
            (45, F(LowerFloor)),
            (38, F(LowerFloorToLowest)),
            (82, F(LowerFloorToLowest)),
            (23, F(LowerFloorToLowest)),
            (60, F(LowerFloorToLowest)),
            (5, F(RaiseFloor)),
            (91, F(RaiseFloor)),
            (101, F(RaiseFloor)),
            (64, F(RaiseFloor)),
            (24, F(RaiseFloor)),
            (30, F(RaiseToTexture)),
            (96, F(RaiseToTexture)),
            (92, F(RaiseFloor24)),
            (59, F(RaiseFloor24andChange)),
            (93, F(RaiseFloor24andChange)),
            (37, F(LowerAndChange)),
            (84, F(LowerAndChange)),
            (36, F(TurboLower)),
            (98, F(TurboLower)),
            (71, F(TurboLower)),
            (70, F(TurboLower)),
            (56, F(RaiseFloorCrush)),
            (94, F(RaiseFloorCrush)),
            (55, F(RaiseFloorCrush)),
            (65, F(RaiseFloorCrush)),
            (119, F(RaiseFloorToNearest)),
            (128, F(RaiseFloorToNearest)),
            (18, F(RaiseFloorToNearest)),
            (69, F(RaiseFloorToNearest)),
            (130, F(RaiseFloorTurbo)),
            (129, F(RaiseFloorTurbo)),
            (131, F(RaiseFloorTurbo)),
            (132, F(RaiseFloorTurbo)),
            (140, F(RaiseFloor512)),
            (9, F(DonutRaise)),
            // ceilings
            (41, C(LowerToFloor)),
            (43, C(LowerToFloor)),
            (25, C(CrushAndRaise)),
            (49, C(CrushAndRaise)),
            (73, C(CrushAndRaise)),
            (6, C(FastCrushAndRaise)),
            (77, C(FastCrushAndRaise)),
            (141, C(SilentCrushAndRaise)),
            (44, C(LowerAndCrush)),
            (72, C(LowerAndCrush)),
            // doors
            (4, D(Normal)),
            (90, D(Normal)),
            (29, D(Normal)),
            (63, D(Normal)),
            (2, D(Open)),
            (86, D(Open)),
            (103, D(Open)),
            (61, D(Open)),
            (46, D(Open)),
            (3, D(Close)),
            (75, D(Close)),
            (50, D(Close)),
            (42, D(Close)),
            (108, D(BlazeRaise)),
            (105, D(BlazeRaise)),
            (111, D(BlazeRaise)),
            (114, D(BlazeRaise)),
            (109, D(BlazeOpen)),
            (106, D(BlazeOpen)),
            (112, D(BlazeOpen)),
            (115, D(BlazeOpen)),
            (110, D(BlazeClose)),
            (107, D(BlazeClose)),
            (113, D(BlazeClose)),
            (116, D(BlazeClose)),
            // manual doors
            (1, D(Normal)),
            (26, D(Normal)),
            (27, D(Normal)),
            (28, D(Normal)),
            (117, D(BlazeRaise)),
            (31, D(Open)),
            (32, D(Open)),
            (33, D(Open)),
            (34, D(Open)),
            (118, D(BlazeOpen)),
            // key switch-doors (BOOM locked range)
            (99, D(BlazeOpen)),
            (133, D(BlazeOpen)),
            (134, D(BlazeOpen)),
            (135, D(BlazeOpen)),
            (136, D(BlazeOpen)),
            (137, D(BlazeOpen)),
            // lifts
            (10, L(DownWaitUpStay)),
            (88, L(DownWaitUpStay)),
            (21, L(DownWaitUpStay)),
            (62, L(DownWaitUpStay)),
            (53, L(PerpetualRaise)),
            (87, L(PerpetualRaise)),
            (121, L(BlazeDWUS)),
            (120, L(BlazeDWUS)),
            (122, L(BlazeDWUS)),
            (123, L(BlazeDWUS)),
            (22, L(RaiseToNearestAndChange)),
            (14, L(RaiseAndChange)),
            // stairs
            (8, S(Build8)),
            (7, S(Build8)),
            (100, S(Turbo16)),
            (127, S(Turbo16)),
        ];
        for &(n, kind) in cases {
            let e = encode_vanilla(n).unwrap();
            let spec = decode(e).unwrap_or_else(|| panic!("special {n} -> {e:#x} did not decode"));
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
            // 46 is gun-REPEATABLE and monster-shootable per OG -> GunMany
            // (0x3C2D) with the door monster bit (0x80).
            (42, 0x3C6B),
            (46, 0x3CAD),
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

    /// Generalized locked doors (`[GEN_LOCKED_BASE, GEN_DOOR_BASE)`) decode as
    /// the Door category using the locked layout (1-bit kind), without the
    /// base-subtraction underflow that used to crash on real maps (sunder
    /// MAP20 uses 0x39AA and 0x3B32).
    #[test]
    fn locked_door_decode() {
        let a = decode(0x39AA).expect("locked door 0x39AA decodes");
        assert_eq!(a.kind, D(Open)); // speed normal, open-stay
        assert_eq!(a.trigger, SwitchOnce);
        assert_eq!(
            a.lock,
            Some(DoorLock {
                kind: LockKind::YellowSkull,
                skulls_are_cards: false,
            })
        );

        let b = decode(0x3B32).expect("locked door 0x3B32 decodes");
        assert_eq!(b.kind, D(BlazeOpen)); // speed fast, open-stay
        assert_eq!(b.trigger, SwitchOnce);
        assert_eq!(
            b.lock,
            Some(DoorLock {
                kind: LockKind::RedSkull,
                skulls_are_cards: true,
            })
        );
    }
}
