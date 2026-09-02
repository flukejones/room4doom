//! Where an aimed teleport lands, and the dash that gets the player there.
//!
//! BSP3D clips the visual arc; ordinary player collision validates the landing
//! and the route from the player's body, independently of the tracked hand.

use glam::{Vec2, Vec3};
use math::{FixedT, MAX_TELEPORT_RANGE, TELEPORT_FLIGHT_SECS, TELEPORT_SPEED, ballistic_point};

use crate::info::{MOBJINFO, MapObjKind};
use crate::level::LevelState;
use crate::thing::MapObject;

/// Points sampled along the arc. Over 5 m that is ~5 units apart, far under the
/// player radius, so no surface can slip between two samples.
const ARC_SAMPLES: u16 = 33;

/// Shortest arc that still reads as one on screen. A zero-length arc draws a
/// degenerate ribbon — nothing at all — so the aim always keeps this much.
const MIN_ARC_SAMPLE: u16 = 2;

const PLAYER_RADIUS: f32 = MOBJINFO[MapObjKind::MT_PLAYER as usize].radius;

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
/// The hand may reach through a wall; it never substitutes for the player's
/// body when deciding whether the destination is reachable.
pub fn aim_teleport(
    level: &LevelState,
    scratch: &mut TeleportScratch,
    origin: Vec3,
    dir: Vec3,
    player: &MapObject,
) -> TeleportAim {
    let invalid = TeleportAim {
        landing: Vec3::new(player.x.to_f32(), player.y.to_f32(), player.z.to_f32()),
        flight_time: 0.0,
        valid: false,
    };
    if !origin.is_finite()
        || !dir.is_finite()
        || !dir.length_squared().is_finite()
        || dir.length_squared() <= f32::EPSILON
        || player.radius <= FixedT::ZERO
    {
        return invalid;
    }
    let step = TELEPORT_FLIGHT_SECS / f32::from(ARC_SAMPLES - 1);
    scratch.points.clear();
    let (mut min, mut max) = (origin, origin);
    for i in 0..ARC_SAMPLES {
        let point = ballistic_point(origin, dir, TELEPORT_SPEED, step * f32::from(i));
        if !point.is_finite() {
            return invalid;
        }
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
        if let Some(landing) = landing_at(level, scratch.points[usize::from(i)], player)
            && let (_, true) = trace_walk(level, landing.truncate(), player)
        {
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

/// Where the player's full collision box fits under `point`.
fn landing_at(level: &LevelState, point: Vec3, player: &MapObject) -> Option<Vec3> {
    let (x, y) = (FixedT::from_f32(point.x), FixedT::from_f32(point.y));
    let clearance = player.position_clearance(level, x, y)?;
    Some(Vec3::new(point.x, point.y, clearance.min_floor_z.to_f32()))
}

/// Furthest walkable point in a dash direction, bounded by ordinary player
/// clearance. A dash into an obstacle can shorten, but cannot cross it.
pub fn dash_target(
    level: &LevelState,
    direction: Vec3,
    distance: f32,
    player: &MapObject,
) -> Option<Vec3> {
    if !direction.is_finite() || !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    let direction = direction.truncate().try_normalize()?;
    let here = Vec2::new(player.x.to_f32(), player.y.to_f32());
    let (reached, _) = trace_walk(level, here + direction * distance, player);
    (reached.truncate().distance_squared(here) > 0.01).then_some(reached)
}

/// Furthest clear position and whether the complete route was reached.
fn trace_walk(level: &LevelState, target: Vec2, player: &MapObject) -> (Vec3, bool) {
    let here = Vec2::new(player.x.to_f32(), player.y.to_f32());
    let delta = target - here;
    let mut reached = Vec3::new(here.x, here.y, player.z.to_f32());
    let Some(steps) = movement_steps(delta, player.radius) else {
        return (reached, false);
    };
    for i in 1..=u16::MAX {
        let progress = f32::from(i) / steps;
        if progress > 1.0 {
            break;
        }
        let point = here + delta * progress;
        let Some(clearance) = player.segment_clearance(
            level,
            (FixedT::from_f32(reached.x), FixedT::from_f32(reached.y)),
            (FixedT::from_f32(point.x), FixedT::from_f32(point.y)),
        ) else {
            break;
        };
        if !clearance.permits_step_from(FixedT::from_f32(reached.z), player.height, player.flags) {
            break;
        }
        reached = Vec3::new(point.x, point.y, clearance.min_floor_z.to_f32());
        if progress >= 1.0 {
            return (reached, true);
        }
    }
    (reached, false)
}

fn movement_steps(delta: Vec2, radius: FixedT) -> Option<f32> {
    let half_radius = radius.to_f32() * 0.5;
    if !delta.is_finite() || !half_radius.is_finite() || half_radius <= 0.0 {
        return None;
    }
    let steps = (delta.length() / half_radius).ceil().max(1.0);
    (steps.is_finite() && steps <= f32::from(u16::MAX)).then_some(steps)
}

/// One tic of the dash toward `target`; true once the player has arrived.
///
/// Recheck small movement steps, so a moving door or monster can stop a dash
/// after its preview. Ordinary movement maintains spatial links and specials.
pub fn step_dash(target: Vec3, tics_left: u16, thing: &mut MapObject) -> bool {
    thing.momx = FixedT::ZERO;
    thing.momy = FixedT::ZERO;
    thing.momz = FixedT::ZERO;
    if !target.is_finite() {
        return true;
    }
    let here = Vec3::new(thing.x.to_f32(), thing.y.to_f32(), thing.z.to_f32());
    let next = if tics_left == 0 {
        target
    } else {
        here + (target - here) / (f32::from(tics_left) + 1.0)
    };

    let Some(steps) = movement_steps((next - here).truncate(), thing.radius) else {
        return true;
    };
    for i in 1..=u16::MAX {
        let progress = f32::from(i) / steps;
        if progress > 1.0 {
            break;
        }
        let point = here + (next - here) * progress;
        let (x, y) = (FixedT::from_f32(point.x), FixedT::from_f32(point.y));
        // SAFETY: the owning level and its blockmap remain alive and stable;
        // this read-only query finishes before ordinary movement mutates them.
        let sweep = unsafe { thing.segment_clearance(&*thing.level, (thing.x, thing.y), (x, y)) };
        if !sweep.is_some_and(|clearance| {
            clearance.permits_step_from(thing.z, thing.height, thing.flags)
        }) {
            return true;
        }
        let mut clearance = Default::default();
        if !thing.p_try_move(x, y, &mut clearance) {
            return true;
        }
        thing.z = thing.floorz;
        if thing.x != x || thing.y != y {
            return true;
        }
    }
    tics_left == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestLevel, rng_guard};

    #[test]
    fn aim_near_wall_keeps_player_clear() {
        let _guard = rng_guard();
        let mut fixture = TestLevel::load("E1M1");
        let player = fixture.spawn_player(1056, -3664);
        let mut scratch = TeleportScratch::default();
        // SAFETY: the fixture owns the player's stable arena slot.
        let player = unsafe { &mut *player };
        let away = aim_teleport(
            fixture.level(),
            &mut scratch,
            Vec3::new(1056.0, -3656.0, 40.0),
            Vec3::new(0.0, 1.0, -0.5).normalize(),
            player,
        );
        assert!(
            away.valid,
            "the same hand can aim safely away from the wall"
        );
        for y in [-3656.0, -3664.0, -3672.0, -3688.0] {
            let origin = Vec3::new(1056.0, y, player.z.to_f32() + 40.0);
            let aim = aim_teleport(
                fixture.level(),
                &mut scratch,
                origin,
                Vec3::new(0.0, -1.0, -0.5).normalize(),
                player,
            );
            let mut position = Default::default();
            let clear = player.p_check_position(
                FixedT::from_f32(aim.landing.x),
                FixedT::from_f32(aim.landing.y),
                &mut position,
            );
            assert!(
                !aim.valid || clear,
                "teleport admits a landing intersecting a wall"
            );
            assert!(
                !aim.valid || aim.landing.y >= -3664.0,
                "hand cannot warp the player through the alcove wall"
            );
        }
    }

    #[test]
    fn dash_stops_at_wall() {
        let _guard = rng_guard();
        let mut fixture = TestLevel::load("E1M1");
        let player = fixture.spawn_player(1056, -3616);
        // SAFETY: the fixture owns the player's stable arena slot.
        let player = unsafe { &mut *player };
        let target = dash_target(fixture.level(), Vec3::NEG_Y, 128.0, player)
            .expect("a dash can move toward the wall");
        assert!(target.y < player.y.to_f32() && target.y >= -3664.0);
        step_dash(Vec3::new(1056.0, -3744.0, 0.0), 0, player);
        assert!(player.y.to_f32() >= -3664.0, "dash crossed the alcove wall");
        let stopped = Vec3::new(player.x.to_f32(), player.y.to_f32(), player.z.to_f32());
        for target in [Vec3::NAN, Vec3::splat(f32::INFINITY)] {
            assert!(step_dash(target, 0, player));
            assert_eq!(
                Vec3::new(player.x.to_f32(), player.y.to_f32(), player.z.to_f32()),
                stopped,
                "malformed dash target moved the player"
            );
        }
    }

    #[test]
    fn dash_cannot_graze_wall_corner() {
        let _guard = rng_guard();
        let mut fixture = TestLevel::load("E1M1");
        let player = fixture.spawn_player(1039, -3631);
        // SAFETY: the fixture owns the player's stable arena slot.
        let player = unsafe { &mut *player };
        // The alcove corner is (1024,-3648). Both endpoint boxes fit, but
        // halfway along this short diagonal the box clips the wall endpoint.
        let start = Vec2::new(1039.0, -3631.5);
        let end = Vec2::new(1040.5, -3633.0);
        let mut ctrl = Default::default();
        assert!(player.p_try_move(
            FixedT::from_f32(start.x),
            FixedT::from_f32(start.y),
            &mut ctrl
        ));
        player.z = player.floorz;
        assert!(player.p_check_position(
            FixedT::from_f32(end.x),
            FixedT::from_f32(end.y),
            &mut ctrl
        ));
        assert!(!player.p_check_position(
            FixedT::from_f32(1039.75),
            FixedT::from_f32(-3632.25),
            &mut ctrl
        ));
        let delta = end - start;
        assert!(
            dash_target(fixture.level(), delta.extend(0.0), delta.length(), player).is_none(),
            "preview missed the corner between endpoints"
        );
        step_dash(end.extend(player.z.to_f32()), 0, player);
        assert_eq!(
            Vec2::new(player.x.to_f32(), player.y.to_f32()),
            start,
            "dash crossed the wall corner"
        );
    }
}
