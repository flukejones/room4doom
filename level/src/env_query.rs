//! Neighbour-sector geometry queries used to compute mover target heights.
//!
//! These are pure walks over a sector's bounding linedefs and their adjacent
//! sectors — no thinker or game state — so they live in `level` and are shared
//! by gameplay (mover dispatch) and tooling (the viewer).
//!
//! Doom source names are `P_Find*Surrounding` in `p_spec`.

use crate::MapPtr;
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Sector, SectorHeight};
use log::{debug, trace};
use std::ptr;

/// `P_GetNextSector`: the sector on the other side of `line` from `sector`, or
/// `None` if the line is one-sided.
pub fn get_next_sector(line: MapPtr<LineDef>, sector: MapPtr<Sector>) -> Option<MapPtr<Sector>> {
    if !line.flags.contains(LineDefFlags::TwoSided) {
        return None;
    }

    if ptr::eq(line.frontsector.as_ref(), sector.as_ref()) {
        return line.backsector.clone();
    }

    Some(line.frontsector.clone())
}

/// `P_FindMinSurroundingLight`
pub fn find_min_light_surrounding(sec: MapPtr<Sector>, max: usize) -> usize {
    let mut min = max;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.lightlevel < min
        {
            min = other.lightlevel;
        }
    }
    trace!("find_min_light_surrounding: {min}");
    min
}

/// `P_FindMaxSurroundingLight`
pub fn find_max_light_surrounding(sec: MapPtr<Sector>, mut max: usize) -> usize {
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.lightlevel > max
        {
            max = other.lightlevel;
        }
    }
    debug!("find_max_light_surrounding: {max}");
    max
}

/// `P_FindLowestCeilingSurrounding`
pub fn find_lowest_ceiling_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut height = SectorHeight::MAX;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.ceilingheight < height
        {
            height = other.ceilingheight;
        }
    }
    debug!("find_lowest_ceiling_surrounding: {height}");
    height
}

/// `P_FindHighestCeilingSurrounding`
pub fn find_highest_ceiling_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut height = SectorHeight::ZERO;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.ceilingheight > height
        {
            height = other.ceilingheight;
        }
    }
    debug!("find_highest_ceiling_surrounding: {height}");
    height
}

/// `P_FindLowestFloorSurrounding`
pub fn find_lowest_floor_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut floor = sec.floorheight;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.floorheight < floor
        {
            floor = other.floorheight;
        }
    }
    debug!("find_lowest_floor_surrounding: {floor}");
    floor
}

/// `P_FindHighestFloorSurrounding`
pub fn find_highest_floor_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut floor = -SectorHeight::MAX;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.floorheight > floor
        {
            floor = other.floorheight;
        }
    }
    debug!("find_highest_floor_surrounding: {floor}");
    floor
}

/// `P_FindNextHighestFloor`: the lowest neighbour floor that is still above
/// `current`.
pub fn find_next_highest_floor(sec: MapPtr<Sector>, current: SectorHeight) -> SectorHeight {
    let mut height_list = Vec::new();

    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone())
            && other.floorheight > current
        {
            height_list.push(other.floorheight);
        }
    }

    if height_list.is_empty() {
        return current;
    }

    let mut min = height_list[0];
    for h in &height_list[1..] {
        if *h < min {
            min = *h;
        }
    }

    min
}
