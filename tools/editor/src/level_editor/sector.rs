//! Sector sampling (which sector a point is in), sector-record edits, and the add/delete/merge sector ops (geometry delegated to the kernel).

use editor_core::geom::sector_at;
use editor_core::{
    ArenaKey as _, Sector, SectorKey, ThingKey, add_sector_in_enclosure, merge_sectors,
    sector_under_cursor_has_separable_loop, sectors_share_two_sided_wall, unmerge_sector_at,
};

use super::LevelEditorState;
use super::pick3d::pick_mesh;
use crate::level_editor::draw::default_sector;
use crate::render::frame3d::{NO_VERT, SURFACE_FLOOR, Vert3D};
use crate::state::{Damage, SelItem};
use crate::undo::EditAction;

/// Above any map ceiling.
const FLOOR_PROBE_Z: f32 = 1.0e6;

impl LevelEditorState {
    pub fn sector_under(&self, world: [f32; 2]) -> Option<SectorKey> {
        sector_at(self.map.as_ref()?, world)
    }

    pub fn selected_sectors(&self) -> Vec<SectorKey> {
        self.selected_of(|i| match i {
            SelItem::Sector(s) => Some(*s),
            _ => None,
        })
    }

    /// Multi-sector selection if any, else the single panel sector, else empty.
    pub fn highlighted_sectors(&self) -> Vec<SectorKey> {
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
            Damage::Edited
        } else {
            Damage::None
        }
    }

    pub fn apply_sector(&mut self, key: SectorKey, sector: Sector) -> Damage {
        let changed = self
            .map
            .as_ref()
            .and_then(|m| m.sectors.get(key))
            .is_some_and(|old| *old != sector);
        if !changed {
            return Damage::None;
        }
        let map = self.map.as_ref().expect("checked above");
        self.undo.record(EditAction::EditSector, map);
        self.set_sector(key, sector)
    }

    /// Write `sector` at `key`; no undo record (composites/sessions snapshot once).
    pub fn set_sector(&mut self, key: SectorKey, sector: Sector) -> Damage {
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        let Some(old) = map.sectors.get(key).copied() else {
            return Damage::None;
        };
        if old == sector {
            return Damage::None;
        }
        let heights_changed =
            old.floor_height != sector.floor_height || old.ceil_height != sector.ceil_height;
        self.dirty = true;
        map.sectors[key] = sector;
        if heights_changed {
            self.rederive_thing_z_in_sector(key);
        }
        Damage::Edited
    }

    fn rederive_thing_z_in_sector(&mut self, s: SectorKey) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let Some((min, max)) = self.sector_bounds(s) else {
            return;
        };
        let floor = map.sectors[s].floor_height;
        let mut candidates = Vec::new();
        self.bvh.things_in_box(min, max, &mut candidates);
        let mesh = &self.surface_mesh;
        let updates: Vec<ThingKey> = candidates
            .into_iter()
            .filter_map(|slot| map.things.key_at_slot(slot))
            .filter(|&k| {
                let t = &map.things[k];
                floor_sector_below([t.x as f32, t.y as f32], mesh) == Some(s.slot())
            })
            .collect();
        let map = self.map.as_mut().expect("checked above");
        for k in updates {
            map.things[k].z = floor;
        }
    }

    fn sector_bounds(&self, s: SectorKey) -> Option<([f32; 3], [f32; 3])> {
        let slot = s.slot();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found = false;
        for v in &self.surface_mesh {
            if v.sector != slot || v.vert == NO_VERT {
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

    pub fn new_sector(&mut self, sector: Sector) -> Option<SectorKey> {
        let map = self.map.as_mut()?;
        self.undo.record(EditAction::EditSector, map);
        self.dirty = true;
        let key = map.sectors.insert(sector);
        self.current_sector = Some(key);
        Some(key)
    }

    pub fn can_add_sector(&self, world: [f32; 2]) -> bool {
        self.sector_under(world).is_none()
    }

    pub fn add_sector_at(&mut self, world: [f32; 2]) -> Damage {
        let Some(map) = self.map.as_ref() else {
            return Damage::None;
        };
        self.undo.record(EditAction::EditSector, map);
        if let Some(new) = self.add_sector(world) {
            self.finish_new_sector(new)
        } else {
            self.undo.discard_last();
            Damage::None
        }
    }

    /// Add a default sector in the void enclosure at `world`; no undo record (paste snapshots once).
    pub(super) fn add_sector(&mut self, world: [f32; 2]) -> Option<SectorKey> {
        let map = self.map.as_mut()?;
        add_sector_in_enclosure(map, world, default_sector())
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
        if let Some(new) = unmerge_sector_at(map, world) {
            self.finish_new_sector(new)
        } else {
            self.undo.discard_last();
            Damage::None
        }
    }

    /// Select the freshly-created sector and mark dirty.
    pub(super) fn finish_new_sector(&mut self, key: SectorKey) -> Damage {
        self.current_sector = Some(key);
        self.selection.retain(|i| !matches!(i, SelItem::Sector(_)));
        self.selection.push(SelItem::Sector(key));
        self.dirty = true;
        Damage::Edited
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
        Damage::Edited
    }
}

/// Sector slot of the floor below `p` via a downward ray through the 3D mesh.
fn floor_sector_below(p: [f32; 2], mesh: &[Vert3D]) -> Option<u32> {
    let origin = [p[0], p[1], FLOOR_PROBE_Z];
    let hit = pick_mesh(mesh, origin, [0.0, 0.0, -1.0])?;
    let v = mesh[hit.tri];
    (v.surface == SURFACE_FLOOR).then_some(v.sector)
}
