//! Where an aimed teleport lands, and the dash that gets the player there.
//!
//! Deliberately geometry-only: the arc is clipped against BSP3D surfaces and
//! the destination checked against the landing sector's thing list, so VR
//! locomotion never enters the blockmap movement path. The cost is that a thing
//! whose centre sits in a neighbouring sector is not seen — that is what the
//! blockmap exists for, and at these distances it is not worth the coupling.

use glam::{Vec2, Vec3};
use math::{FixedT, MAX_TELEPORT_RANGE, TELEPORT_FLIGHT_SECS, TELEPORT_SPEED, ballistic_point};

use crate::info::{MOBJINFO, MapObjKind};
use crate::level::LevelState;
use crate::sector_ext::SectorExt as _;
use crate::thing::{MapObjFlag, MapObject};

/// Points sampled along the arc. Over 5 m that is ~5 units apart, far under the
/// player radius, so no surface can slip between two samples.
const ARC_SAMPLES: u16 = 33;

/// Shortest arc that still reads as one on screen. A zero-length arc draws a
/// degenerate ribbon — nothing at all — so the aim always keeps this much.
const MIN_ARC_SAMPLE: u16 = 2;

const PLAYER_RADIUS: f32 = MOBJINFO[MapObjKind::MT_PLAYER as usize].radius;
const PLAYER_HEIGHT: f32 = MOBJINFO[MapObjKind::MT_PLAYER as usize].height;

/// Tics the dash to the landing spot takes — ~114 ms at 35 tics, fast enough to
/// avoid vection while still reading as movement rather than a cut.
pub const TELEPORT_TICS: u16 = 4;

/// A teleport in flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportDash {
    pub target: Vec3,
    pub tics: u16,
}

/// The resolved aim for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportAim {
    /// Where the player would stand, feet on the floor.
    pub landing: Vec3,
    /// Seconds along the arc the landing sits at, so the draw stops there.
    pub flight_time: f32,
    pub valid: bool,
}

/// Reused between frames so aiming allocates nothing.
#[derive(Default)]
pub struct TeleportScratch {
    leaves: Vec<usize>,
    points: Vec<Vec3>,
}

/// The aim arc from `origin` along `dir`, resolved to where it puts the player.
///
/// `player` is excluded from the thing test — they are in the landing sector's
/// own list whenever they aim near their feet.
pub fn aim_teleport(
    level: &LevelState,
    scratch: &mut TeleportScratch,
    origin: Vec3,
    dir: Vec3,
    player: &MapObject,
) -> TeleportAim {
    let step = TELEPORT_FLIGHT_SECS / f32::from(ARC_SAMPLES - 1);
    scratch.points.clear();
    let (mut min, mut max) = (origin, origin);
    for i in 0..ARC_SAMPLES {
        let point = ballistic_point(origin, dir, TELEPORT_SPEED, step * f32::from(i));
        min = min.min(point);
        max = max.max(point);
        scratch.points.push(point);
        if point.truncate().distance(origin.truncate()) > MAX_TELEPORT_RANGE {
            break;
        }
    }

    let bsp = level.level_data.bsp_3d();
    let pad = Vec3::splat(PLAYER_RADIUS);
    scratch.leaves.clear();
    bsp.leaves_overlapping(min - pad, max + pad, &mut scratch.leaves);

    // Samples still inside the player's own radius cannot block: the hand
    // reaches out to within touching distance of walls and floors as a matter
    // of course, and testing them would collapse every arc to nothing.
    let clear_of_body = scratch
        .points
        .iter()
        .position(|p| p.distance(origin) >= PLAYER_RADIUS)
        .unwrap_or(scratch.points.len());
    let blocked = scratch.points[clear_of_body..]
        .iter()
        .position(|&p| {
            bsp.blocking_poly_near(p, PLAYER_RADIUS, &scratch.leaves)
                .is_some()
        })
        .map(|i| i + clear_of_body);
    let reached = blocked.map_or(scratch.points.len() - 1, |i| i.saturating_sub(1));
    let reached = u16::try_from(reached).expect("at most ARC_SAMPLES points are ever pushed");

    for i in (MIN_ARC_SAMPLE..=reached.max(MIN_ARC_SAMPLE)).rev() {
        if let Some(landing) = landing_at(level, scratch.points[usize::from(i)], player) {
            return TeleportAim {
                landing,
                flight_time: step * f32::from(i),
                valid: true,
            };
        }
    }
    let shown = reached.max(MIN_ARC_SAMPLE);
    TeleportAim {
        landing: scratch.points[usize::from(shown).min(scratch.points.len() - 1)],
        flight_time: step * f32::from(shown),
        valid: false,
    }
}

/// Where a player would stand under `point`, or `None` when they could not.
fn landing_at(level: &LevelState, point: Vec3, player: &MapObject) -> Option<Vec3> {
    let (x, y) = (FixedT::from_f32(point.x), FixedT::from_f32(point.y));
    let bsp = level.level_data.bsp_3d();
    let leaf = bsp.get_leaf(bsp.point_in_leaf(x, y))?;
    let floor = leaf.sector.floor_z(x, y).to_f32();
    if leaf.sector.ceil_z(x, y).to_f32() - floor < PLAYER_HEIGHT {
        return None;
    }
    let stand = Vec3::new(point.x, point.y, floor);
    leaf.sector
        .run_func_on_thinglist(|thing| std::ptr::eq(thing, player) || !blocks_landing(thing, stand))
        .then_some(stand)
}

/// Whether a solid thing occupies the space the player would stand in.
fn blocks_landing(thing: &MapObject, stand: Vec3) -> bool {
    if !thing.flags.contains(MapObjFlag::Solid) {
        return false;
    }
    let (thing_z, thing_top) = (thing.z.to_f32(), thing.z.to_f32() + thing.height.to_f32());
    if thing_z >= stand.z + PLAYER_HEIGHT || thing_top <= stand.z {
        return false;
    }
    let gap = PLAYER_RADIUS + thing.radius.to_f32();
    let apart = Vec2::new(thing.x.to_f32() - stand.x, thing.y.to_f32() - stand.y);
    apart.length_squared() < gap * gap
}

/// One tic of the dash toward `target`; true once the player has arrived.
///
/// The arc proved the path clear, so no collision test runs here. The sector
/// relink still happens each step, or specials and sound would not follow.
pub fn step_dash(
    target: Vec3,
    tics_left: u16,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    let here = Vec3::new(thing.x.to_f32(), thing.y.to_f32(), thing.z.to_f32());
    let next = if tics_left == 0 {
        target
    } else {
        here + (target - here) / f32::from(tics_left + 1)
    };

    let (x, y) = (FixedT::from_f32(next.x), FixedT::from_f32(next.y));
    let subsect = &mut *level.level_data.point_in_subsector(x, y);
    let floorz = subsect.sector.floor_z(x, y);
    let ceilingz = subsect.sector.ceil_z(x, y);
    // SAFETY: the thing is linked into the level being stepped, which is what
    // the position calls require.
    unsafe {
        thing.unset_thing_position();
        thing.x = x;
        thing.y = y;
        thing.z = FixedT::from_f32(next.z);
        thing.floorz = floorz;
        thing.ceilingz = ceilingz;
        thing.set_thing_position();
    }
    tics_left == 0
}
