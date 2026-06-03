//! Click-to-toggle mover animation for the 3D view.
//!
//! On click we ray-pick a sector; if it is a mover we ask `level` for its
//! target heights and lerp the affected surfaces there (and back on a second
//! click). No gameplay crate, no thinkers — just `move_surface` per frame.

use std::collections::HashMap;

use level::LevelData;
use level::MovementType;
use level::env_target::mover_targets_for_sector;

/// Travel speed of a lerping surface, in map units per second.
const LERP_SPEED: f32 = 200.0;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    ToTarget,
    ToRest,
}

struct SurfaceAnim {
    sector_id: usize,
    movement: MovementType,
    rest: f32,
    target: f32,
    current: f32,
    texture: usize,
    phase: Phase,
}

/// Per-clicked-sector group of animating surfaces.
#[derive(Default)]
pub struct MoverState {
    /// Keyed by the clicked sector; each entry is the surface group it drives.
    groups: HashMap<usize, Vec<SurfaceAnim>>,
}

impl MoverState {
    /// Toggle the mover under `clicked_sector`: start moving to target, or if
    /// already triggered, reverse back to rest. Returns `true` if it acted on a
    /// mover (so the caller can stop trying other candidate sectors).
    ///
    /// A group is keyed by the trigger sector, but a re-click may resolve to a
    /// different candidate (e.g. you now click the moved surface itself), so we
    /// also match against the sectors each group actually moves.
    pub fn toggle(&mut self, clicked_sector: usize, level: &mut LevelData) -> bool {
        if let Some(group) = self
            .groups
            .values_mut()
            .find(|g| g.iter().any(|s| s.sector_id == clicked_sector))
        {
            for s in group.iter_mut() {
                s.phase = match s.phase {
                    Phase::ToTarget => Phase::ToRest,
                    Phase::ToRest => Phase::ToTarget,
                };
            }
            return true;
        }

        let targets = mover_targets_for_sector(clicked_sector, level, &|_| 0);
        if targets.is_empty() {
            return false;
        }
        let group: Vec<SurfaceAnim> = targets
            .into_iter()
            .map(|t| {
                let sec = &level.sectors[t.sector_id];
                let rest = match t.movement {
                    MovementType::Ceiling => sec.ceilingheight.to_f32(),
                    _ => sec.floorheight.to_f32(),
                };
                let texture = match t.movement {
                    MovementType::Ceiling => sec.ceilingpic,
                    _ => sec.floorpic,
                };
                SurfaceAnim {
                    sector_id: t.sector_id,
                    movement: t.movement,
                    rest,
                    target: t.height,
                    current: rest,
                    texture,
                    phase: Phase::ToTarget,
                }
            })
            .collect();
        self.groups.insert(clicked_sector, group);
        true
    }

    /// Advance all active lerps by `dt` seconds, applying each to the BSP3D.
    /// Returns true if any surface is still moving (so the view keeps
    /// repainting).
    pub fn tick(&mut self, level: &mut LevelData, dt: f32) -> bool {
        let step = LERP_SPEED * dt;
        let mut active = false;
        for group in self.groups.values_mut() {
            for s in group.iter_mut() {
                let goal = match s.phase {
                    Phase::ToTarget => s.target,
                    Phase::ToRest => s.rest,
                };
                if (s.current - goal).abs() > f32::EPSILON {
                    let delta = (goal - s.current).clamp(-step, step);
                    s.current += delta;
                    if (s.current - goal).abs() < 0.01 {
                        s.current = goal;
                    }
                    level
                        .bsp_3d
                        .move_surface(s.sector_id, s.movement, s.current, s.texture);
                    if s.current != goal {
                        active = true;
                    }
                }
            }
        }
        active
    }

    /// Any group is mid-animation or settled-at-target (so a second click can
    /// reverse it).
    pub fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }
}
