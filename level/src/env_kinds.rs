//! Mover kind vocabulary: the floor/ceiling/door/platform/stair kinds the
//! engine and tooling share. These are level-environment types (no game state),
//! moved here from gameplay so target-height computation can live in `level`.
//!
//! The `#[repr(u8)]` discriminant order is load-bearing: it is the `kind` field
//! carried by the generalized encoding (see [`crate::special_encode`]).

/// Floor mover kinds (`p_floor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FloorKind {
    /// lower floor to highest surrounding floor
    LowerFloor,
    /// lower floor to lowest surrounding floor
    LowerFloorToLowest,
    /// lower floor to highest surrounding floor VERY FAST
    TurboLower,
    /// raise floor to lowest surrounding CEILING
    RaiseFloor,
    /// raise floor to next highest surrounding floor
    RaiseFloorToNearest,
    /// raise floor to shortest height with same texture around it
    RaiseToTexture,
    /// lower floor to lowest surrounding floor and change floorpic
    LowerAndChange,
    /// Raise floor 24 units from start
    RaiseFloor24,
    /// Raise floor 24 units from start and change texture
    RaiseFloor24andChange,
    /// Raise floor and crush all entities on it
    RaiseFloorCrush,
    /// raise to next highest floor, turbo-speed
    RaiseFloorTurbo,
    /// Do donuts
    DonutRaise,
    /// Raise floor 512 units from start
    RaiseFloor512,
}

impl TryFrom<u8> for FloorKind {
    /// The raw byte that failed to map to a variant.
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(FloorKind::LowerFloor),
            1 => Ok(FloorKind::LowerFloorToLowest),
            2 => Ok(FloorKind::TurboLower),
            3 => Ok(FloorKind::RaiseFloor),
            4 => Ok(FloorKind::RaiseFloorToNearest),
            5 => Ok(FloorKind::RaiseToTexture),
            6 => Ok(FloorKind::LowerAndChange),
            7 => Ok(FloorKind::RaiseFloor24),
            8 => Ok(FloorKind::RaiseFloor24andChange),
            9 => Ok(FloorKind::RaiseFloorCrush),
            10 => Ok(FloorKind::RaiseFloorTurbo),
            11 => Ok(FloorKind::DonutRaise),
            12 => Ok(FloorKind::RaiseFloor512),
            _ => Err(v),
        }
    }
}

/// Stair build kinds (`p_floor` EV_BuildStairs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StairKind {
    /// slowly build by 8
    Build8,
    /// quickly build by 16
    Turbo16,
}

impl TryFrom<u8> for StairKind {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(StairKind::Build8),
            1 => Ok(StairKind::Turbo16),
            _ => Err(v),
        }
    }
}

/// Ceiling mover kinds (`p_ceilng`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CeilKind {
    LowerToFloor,
    RaiseToHighest,
    LowerAndCrush,
    CrushAndRaise,
    FastCrushAndRaise,
    SilentCrushAndRaise,
}

impl TryFrom<u8> for CeilKind {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(CeilKind::LowerToFloor),
            1 => Ok(CeilKind::RaiseToHighest),
            2 => Ok(CeilKind::LowerAndCrush),
            3 => Ok(CeilKind::CrushAndRaise),
            4 => Ok(CeilKind::FastCrushAndRaise),
            5 => Ok(CeilKind::SilentCrushAndRaise),
            _ => Err(v),
        }
    }
}

/// Door mover kinds (`p_doors`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DoorKind {
    Normal,
    Close30ThenOpen,
    Close,
    Open,
    RaiseIn5Mins,
    BlazeRaise,
    BlazeOpen,
    BlazeClose,
}

impl TryFrom<u8> for DoorKind {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(DoorKind::Normal),
            1 => Ok(DoorKind::Close30ThenOpen),
            2 => Ok(DoorKind::Close),
            3 => Ok(DoorKind::Open),
            4 => Ok(DoorKind::RaiseIn5Mins),
            5 => Ok(DoorKind::BlazeRaise),
            6 => Ok(DoorKind::BlazeOpen),
            7 => Ok(DoorKind::BlazeClose),
            _ => Err(v),
        }
    }
}

/// Platform / lift mover kinds (`p_plats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlatKind {
    PerpetualRaise,
    DownWaitUpStay,
    RaiseAndChange,
    RaiseToNearestAndChange,
    BlazeDWUS,
}

impl TryFrom<u8> for PlatKind {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(PlatKind::PerpetualRaise),
            1 => Ok(PlatKind::DownWaitUpStay),
            2 => Ok(PlatKind::RaiseAndChange),
            3 => Ok(PlatKind::RaiseToNearestAndChange),
            4 => Ok(PlatKind::BlazeDWUS),
            _ => Err(v),
        }
    }
}
