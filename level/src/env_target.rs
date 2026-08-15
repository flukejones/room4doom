//! Pure mover target-height computation: the single source of destination
//! math, used by gameplay to seed mover thinkers and by tooling to preview a
//! mover's final position without running the game.

use crate::env_query::{
    find_highest_ceiling_surrounding, find_highest_floor_surrounding,
    find_lowest_ceiling_surrounding, find_lowest_floor_surrounding, find_next_highest_floor,
};
use crate::flags::LineDefFlags;
use crate::level_data::LevelData;
use crate::map_defs::{Sector, SectorHeight};
use crate::special_encode::{self, CeilKind, DoorKind, FloorKind, MoverKind, PlatKind};
use crate::{MapPtr, MovementType};

/// Floor mover destination height. `tex_min` is the shortest surrounding lower
/// texture height (only consulted for `RaiseToTexture`; pass 0 otherwise).
pub fn floor_target(sec: MapPtr<Sector>, kind: FloorKind, tex_min: SectorHeight) -> SectorHeight {
    match kind {
        FloorKind::LowerFloor => find_highest_floor_surrounding(sec),
        FloorKind::TurboLower => {
            // OG lands 8 above the neighbour floor unless already level.
            let dest = find_highest_floor_surrounding(sec.clone());
            if dest == sec.floorheight {
                dest
            } else {
                dest + 8
            }
        }
        FloorKind::LowerFloorToLowest | FloorKind::LowerAndChange => {
            find_lowest_floor_surrounding(sec)
        }
        FloorKind::RaiseFloor => {
            let dest = find_lowest_ceiling_surrounding(sec.clone());
            if dest > sec.ceilingheight {
                sec.ceilingheight
            } else {
                dest
            }
        }
        FloorKind::RaiseFloorCrush => {
            let mut dest = find_lowest_ceiling_surrounding(sec.clone());
            if dest > sec.ceilingheight {
                dest = sec.ceilingheight;
            }
            dest - 8
        }
        FloorKind::RaiseFloorToNearest | FloorKind::RaiseFloorTurbo => {
            find_next_highest_floor(sec.clone(), sec.floorheight)
        }
        FloorKind::RaiseToTexture => sec.floorheight + tex_min,
        FloorKind::RaiseFloor24 | FloorKind::RaiseFloor24andChange => sec.floorheight + 24,
        FloorKind::RaiseFloor512 => sec.floorheight + 512,
        // DonutRaise targets are computed per affected sector elsewhere.
        FloorKind::DonutRaise => sec.floorheight,
    }
}

/// Ceiling mover (top, bottom) heights and travel direction (`1` up, `-1`
/// down).
pub fn ceiling_target(sec: MapPtr<Sector>, kind: CeilKind) -> (SectorHeight, SectorHeight, i32) {
    match kind {
        CeilKind::LowerToFloor => (sec.ceilingheight, sec.floorheight, -1),
        CeilKind::RaiseToHighest => (
            find_highest_ceiling_surrounding(sec.clone()),
            sec.floorheight,
            1,
        ),
        CeilKind::LowerAndCrush
        | CeilKind::CrushAndRaise
        | CeilKind::FastCrushAndRaise
        | CeilKind::SilentCrushAndRaise => (sec.ceilingheight, sec.floorheight + 8, -1),
    }
}

/// The opened-door ceiling height: 4 under the lowest surrounding ceiling.
pub fn door_top(sec: MapPtr<Sector>) -> SectorHeight {
    find_lowest_ceiling_surrounding(sec) - 4
}

/// Door destination height. `Close*` doors target the floor.
pub fn door_target(sec: MapPtr<Sector>, kind: DoorKind) -> SectorHeight {
    match kind {
        DoorKind::Close | DoorKind::BlazeClose => sec.floorheight,
        DoorKind::Close30ThenOpen => sec.ceilingheight,
        _ => door_top(sec),
    }
}

/// Platform/lift `(low, high)` travel bounds. `amount` is the RaiseAndChange
/// height delta (24 or 32).
pub fn plat_target(
    sec: MapPtr<Sector>,
    kind: PlatKind,
    amount: i32,
) -> (SectorHeight, SectorHeight) {
    match kind {
        PlatKind::DownWaitUpStay | PlatKind::BlazeDWUS => {
            let mut low = find_lowest_floor_surrounding(sec.clone());
            if low > sec.floorheight {
                low = sec.floorheight;
            }
            (low, sec.floorheight)
        }
        PlatKind::PerpetualRaise => {
            let mut low = find_lowest_floor_surrounding(sec.clone());
            if low > sec.floorheight {
                low = sec.floorheight;
            }
            let mut high = find_highest_floor_surrounding(sec.clone());
            if high < sec.floorheight {
                high = sec.floorheight;
            }
            (low, high)
        }
        PlatKind::RaiseAndChange => (sec.floorheight, sec.floorheight + amount),
        PlatKind::RaiseToNearestAndChange => (
            sec.floorheight,
            find_next_highest_floor(sec.clone(), sec.floorheight),
        ),
    }
}

/// A surface that a triggered mover would move, with its destination height.
#[derive(Debug)]
pub struct MoverTarget {
    pub sector_id: usize,
    pub movement: MovementType,
    pub height: f32,
}

/// Find the linedef that triggers movement of `sector_id`: a tag-matched mover
/// line, or a manual-door line whose back sector is this sector.
fn trigger_special(level: &LevelData, sector_id: usize) -> Option<u32> {
    let tag = level.sectors[sector_id].tag;
    if tag != 0 {
        for line in level.linedefs.iter() {
            if line.tag == tag && special_encode::decode(line.special).is_some() {
                return Some(line.special);
            }
        }
    }
    for line in level.linedefs.iter() {
        if let Some(back) = line.backsector.as_ref()
            && back.num as usize == sector_id
            && special_encode::decode(line.special).is_some_and(|s| s.manual)
        {
            return Some(line.special);
        }
    }
    None
}

/// Shortest surrounding lower-texture height for `RaiseToTexture`.
///
/// `tex_height` maps a texture index to its height in map units (the caller
/// supplies this from its texture data; `level` has no texture sizes). Seeded
/// with the sector's own floor height, matching the engine (OG seeds with
/// MAXINT; a long-standing deliberate divergence).
pub fn shortest_texture(sec: &Sector, tex_height: &dyn Fn(usize) -> i32) -> SectorHeight {
    let mut min = sec.floorheight;
    for line in &sec.lines {
        if !line.flags.contains(LineDefFlags::TwoSided) {
            continue;
        }
        if let Some(t) = line.front_sidedef.bottomtexture {
            min = min.min(SectorHeight::from(tex_height(t)));
        }
        if let Some(side) = line.back_sidedef.as_ref()
            && let Some(t) = side.bottomtexture
        {
            min = min.min(SectorHeight::from(tex_height(t)));
        }
    }
    min
}

/// For a clicked sector, return every surface a triggered mover would move and
/// its destination height. Empty if the sector is not a mover.
///
/// `tex_height` supplies texture heights for `RaiseToTexture` (pass `|_| 0`
/// when not needed).
pub fn mover_targets_for_sector(
    sector_id: usize,
    level: &mut LevelData,
    tex_height: &dyn Fn(usize) -> i32,
) -> Vec<MoverTarget> {
    let Some(special) = trigger_special(level, sector_id) else {
        return Vec::new();
    };
    let Some(spec) = special_encode::decode(special) else {
        return Vec::new();
    };

    // Tag movers move every sector sharing the tag; manual doors move just the
    // clicked sector.
    let tag = level.sectors[sector_id].tag;
    let targets: Vec<usize> = if tag != 0 && !spec.manual {
        level
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tag == tag)
            .map(|(i, _)| i)
            .collect()
    } else {
        vec![sector_id]
    };

    let mut out = Vec::new();
    for sid in targets {
        let sec = MapPtr::new(&mut level.sectors[sid]);
        if spec.composite {
            // vanilla 40: ceiling raises to highest, floor lowers to lowest.
            let (top, ..) = ceiling_target(sec.clone(), CeilKind::RaiseToHighest);
            out.push(MoverTarget {
                sector_id: sid,
                movement: MovementType::Ceiling,
                height: top.to_f32(),
            });
            let dest = floor_target(sec, FloorKind::LowerFloorToLowest, SectorHeight::ZERO);
            out.push(MoverTarget {
                sector_id: sid,
                movement: MovementType::Floor,
                height: dest.to_f32(),
            });
            continue;
        }
        match spec.kind {
            // Donut targets depend on the pool's neighbours and stairs build a
            // chain from the seed sector; neither is previewed.
            MoverKind::Floor(FloorKind::DonutRaise) | MoverKind::Stairs(_) => {}
            MoverKind::Floor(kind) => {
                let tex_min = if matches!(kind, FloorKind::RaiseToTexture) {
                    shortest_texture(&sec, tex_height)
                } else {
                    SectorHeight::ZERO
                };
                let dest = floor_target(sec, kind, tex_min);
                out.push(MoverTarget {
                    sector_id: sid,
                    movement: MovementType::Floor,
                    height: dest.to_f32(),
                });
            }
            MoverKind::Ceiling(kind) => {
                let (top, bottom, dir) = ceiling_target(sec, kind);
                let height = if dir > 0 { top } else { bottom };
                out.push(MoverTarget {
                    sector_id: sid,
                    movement: MovementType::Ceiling,
                    height: height.to_f32(),
                });
            }
            MoverKind::Door(kind) => {
                let dest = door_target(sec, kind);
                out.push(MoverTarget {
                    sector_id: sid,
                    movement: MovementType::Ceiling,
                    height: dest.to_f32(),
                });
            }
            MoverKind::Lift(kind) => {
                let (low, high) = plat_target(sec, kind, spec.amount);
                // Lifts move the floor; the "active" position is the low end
                // (lifts/blaze) or high end (raise-and-change).
                let height = match kind {
                    PlatKind::RaiseAndChange | PlatKind::RaiseToNearestAndChange => high,
                    _ => low,
                };
                out.push(MoverTarget {
                    sector_id: sid,
                    movement: MovementType::Floor,
                    height: height.to_f32(),
                });
            }
        }
    }
    out
}
