//! Sector sampling (which sector a point is in), sector-record edits, and the
//! add/delete/merge sector ops (geometry delegated to the kernel).

use editor_core::geom::sector_at;
use editor_core::{
    Sector, add_sector_in_enclosure, merge_sectors, sector_under_cursor_has_separable_loop,
    sectors_share_two_sided_wall, unmerge_sector_at,
};

use super::LevelEditorState;
use super::pick3d::pick_mesh;
use crate::level_editor::draw::default_sector;
use crate::render::frame3d::{SURFACE_FLOOR, Vert3D};
use crate::state::{ChangedElems, Damage, SelItem};
use crate::undo::EditAction;

/// Above any map ceiling.
const FLOOR_PROBE_Z: f32 = 1.0e6;

impl LevelEditorState {
    pub fn sector_under(&self, world: [f32; 2]) -> Option<u32> {
        sector_at(self.map.as_ref()?, world)
    }

    pub fn selected_sectors(&self) -> Vec<u32> {
        self.selected_of(|i| match i {
            SelItem::Sector(s) => Some(*s),
            _ => None,
        })
    }

    /// Multi-sector selection if any, else the single panel sector, else empty.
    pub fn highlighted_sectors(&self) -> Vec<u32> {
        let sel = self.selected_sectors();
        if sel.is_empty() {
            self.current_sector.into_iter().collect()
        } else {
            sel
        }
    }

    pub(super) fn sample_sector(&mut self, world: [f32; 2]) -> Damage {
        let prev_sector = self.current_sector;
        let prev_selection = !self.selection.is_empty();
        if let Some(sector) = self.sector_under(world) {
            self.current_sector = Some(sector);
            self.sampled_sector = Some(sector);
        } else {
            self.current_sector = None;
            self.selection.clear();
        }
        if self.current_sector != prev_sector || (prev_selection && self.selection.is_empty()) {
            Damage::Geometry
        } else {
            Damage::None
        }
    }

    pub fn apply_sector(&mut self, index: u32, sector: Sector) -> Damage {
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        let Some(old) = map.sectors.get(index as usize).copied() else {
            return Damage::None;
        };
        if old == sector {
            return Damage::None;
        }
        let heights_changed =
            old.floor_height != sector.floor_height || old.ceil_height != sector.ceil_height;
        let flat_changed = old.floor_flat != sector.floor_flat || old.ceil_flat != sector.ceil_flat;
        self.undo.record(EditAction::EditSector, map);
        self.dirty = true;
        let Some(slot) = map.sectors.get_mut(index as usize) else {
            return Damage::None;
        };
        *slot = sector;
        if heights_changed {
            self.rederive_thing_z_in_sector(index);
            Damage::Geometry
        } else if flat_changed {
            Damage::Patch(ChangedElems::sector_flat(index))
        } else {
            Damage::Patch(ChangedElems::sector(index))
        }
    }

    fn rederive_thing_z_in_sector(&mut self, s: u32) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let Some((min, max)) = self.sector_bounds(s) else {
            return;
        };
        let floor = map.sectors[s as usize].floor_height;
        let mut candidates = Vec::new();
        self.bvh.things_in_box(min, max, &mut candidates);
        let mesh = &self.surface_mesh;
        let updates: Vec<u32> = candidates
            .into_iter()
            .filter(|&id| {
                map.things
                    .get(id as usize)
                    .is_some_and(|t| floor_sector_below([t.x as f32, t.y as f32], mesh) == Some(s))
            })
            .collect();
        let map = self.map.as_mut().expect("checked above");
        for id in updates {
            map.things[id as usize].z = floor;
        }
    }

    fn sector_bounds(&self, s: u32) -> Option<([f32; 3], [f32; 3])> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found = false;
        for v in &self.surface_mesh {
            if v.sector != s {
                continue;
            }
            found = true;
            for a in 0..3 {
                min[a] = min[a].min(v.pos[a]);
                max[a] = max[a].max(v.pos[a]);
            }
        }
        found.then_some((min, max))
    }

    pub fn new_sector(&mut self, sector: Sector) -> Option<u32> {
        let map = self.map.as_mut()?;
        self.undo.record(EditAction::EditSector, map);
        self.dirty = true;
        map.sectors.push(sector);
        let index = (map.sectors.len() - 1) as u32;
        self.current_sector = Some(index);
        Some(index)
    }

    pub fn can_add_sector(&self, world: [f32; 2]) -> bool {
        self.sector_under(world).is_none()
    }

    pub fn add_sector_at(&mut self, world: [f32; 2]) -> Damage {
        let record = default_sector();
        let Some(map) = self.map.as_mut() else {
            return Damage::None;
        };
        self.undo.record(EditAction::EditSector, map);
        let new = add_sector_in_enclosure(map, world, record);
        self.finish_new_sector(new)
    }

    pub fn can_unmerge_sector(&self, world: [f32; 2]) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| sector_under_cursor_has_separable_loop(m, world))
    }

    pub fn unmerge_sector(&mut self, world: [f32; 2]) -> Damage {
        let Some(map) = self.map.as_mut() else {
            return Damage::None;
        };
        self.undo.record(EditAction::EditSector, map);
        let new = unmerge_sector_at(map, world);
        self.finish_new_sector(new)
    }

    fn finish_new_sector(&mut self, new: Option<u32>) -> Damage {
        let Some(index) = new else {
            self.undo.discard_last();
            return Damage::None;
        };
        self.current_sector = Some(index);
        self.selection.retain(|i| !matches!(i, SelItem::Sector(_)));
        self.selection.push(SelItem::Sector(index));
        self.dirty = true;
        Damage::Geometry
    }

    pub fn delete_active(&mut self) -> Damage {
        self.delete_selection()
    }

    pub fn can_merge_sectors(&self) -> bool {
        let sel = self.selected_sectors();
        sel.len() == 2
            && self
                .map
                .as_ref()
                .is_some_and(|m| sectors_share_two_sided_wall(m, sel[0], sel[1]))
    }

    pub fn merge_selected_sectors(&mut self) -> Damage {
        if !self.can_merge_sectors() {
            return Damage::None;
        }
        let sel = self.selected_sectors();
        let pair = (sel[0], sel[1]);
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditSector, map);
        merge_sectors(map, &[pair]);
        self.current_sector = Some(pair.0.min(pair.1));
        self.selection.retain(|i| !matches!(i, SelItem::Sector(_)));
        self.dirty = true;
        Damage::Geometry
    }
}

/// Sector of the floor below `p` via a downward ray through the 3D mesh.
fn floor_sector_below(p: [f32; 2], mesh: &[Vert3D]) -> Option<u32> {
    let origin = [p[0], p[1], FLOOR_PROBE_Z];
    let hit = pick_mesh(mesh, origin, [0.0, 0.0, -1.0])?;
    let v = mesh[hit.tri];
    (v.surface == SURFACE_FLOOR).then_some(v.sector)
}
