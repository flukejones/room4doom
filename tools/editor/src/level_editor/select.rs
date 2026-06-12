//! Selection: click-select, rubber-band lasso, and the multi-sector selection.

use editor_core::weld_cluster;

use super::{LevelEditorState, VERTEX_WELD_DIST};
use crate::boundary::SelectMode;
use crate::level_editor::draw::default_sector;
use crate::level_editor::pick3d::PickKind;
use crate::render::camera3d::Mat4;
use crate::state::{ChangedElems, Damage, DragState, Overlay, SectorFill, SelItem};
use crate::undo::EditAction;

/// Sector → full rebuild; other kinds patch their slot only.
fn selection_damage(items: impl IntoIterator<Item = SelItem>) -> Damage {
    let mut changed = ChangedElems::default();
    for item in items {
        match item {
            SelItem::Vertex(i) => changed.verts.push(i),
            SelItem::Line(i) => changed.lines.push(i),
            SelItem::Thing(i) => changed.things.push(i),
            SelItem::Sector(_) => return Damage::Geometry,
        }
    }
    if changed == ChangedElems::default() {
        Damage::None
    } else {
        Damage::Patch(changed)
    }
}

impl LevelEditorState {
    pub fn clear_selection(&mut self) -> Damage {
        let cleared: Vec<SelItem> = self.selection.items().to_vec();
        let had_sector = self.current_sector.is_some();
        self.selection.clear();
        self.current_sector = None;
        // No selection → grid resets to z=0.
        let plane_reset = self.set_grid_plane(0.0);
        if had_sector {
            return Damage::Geometry;
        }
        let damage = self.selection_damage_for_mode(cleared);
        if plane_reset && matches!(damage, Damage::None) {
            Damage::View
        } else {
            damage
        }
    }

    /// In `None` fill mode, `Patch` escalates to rebuild (highlight rides the static wire layer).
    fn selection_damage_for_mode(&self, touched: Vec<SelItem>) -> Damage {
        let damage = selection_damage(touched);
        if self.sector_fill == SectorFill::None && matches!(damage, Damage::Patch(_)) {
            return Damage::Geometry;
        }
        damage
    }

    pub(super) fn select_pick(&mut self, kind: PickKind, shift: bool) -> Damage {
        let mut touched: Vec<SelItem> = self.selection.items().to_vec();
        let prev_sector = self.current_sector;
        match kind {
            PickKind::Vertex(i) => self.toggle_or_replace(SelItem::Vertex(i), shift, &mut touched),
            PickKind::Linedef(i) => self.toggle_or_replace(SelItem::Line(i), shift, &mut touched),
            PickKind::Thing(i) => self.toggle_or_replace(SelItem::Thing(i), shift, &mut touched),
            PickKind::Sector(s) => {
                self.current_sector = if shift && Some(s) == self.current_sector {
                    None
                } else {
                    Some(s)
                };
                self.update_selected_sectors(Some(s), shift);
            }
        }
        if self.current_sector != prev_sector {
            return Damage::Geometry;
        }
        touched.extend(self.selection.items().iter().copied());
        self.selection_damage_for_mode(touched)
    }

    fn toggle_or_replace(&mut self, item: SelItem, shift: bool, touched: &mut Vec<SelItem>) {
        self.current_sector = None;
        if shift {
            self.selection.toggle(item);
        } else {
            self.selection.replace(item);
        }
        touched.push(item);
    }

    fn update_selected_sectors(&mut self, under: Option<u32>, shift: bool) {
        if !shift {
            self.selection.retain(|i| !matches!(i, SelItem::Sector(_)));
        }
        let Some(s) = under else { return };
        if shift {
            self.selection.toggle(SelItem::Sector(s));
        } else {
            self.selection.push(SelItem::Sector(s));
        }
    }

    pub(super) fn begin_select_drag(
        &mut self,
        pos: [f32; 2],
        mode: SelectMode,
        world: [f32; 2],
        shift: bool,
    ) -> Damage {
        if self.map.is_none() {
            return Damage::None;
        }
        if let Some(item) = self
            .pick_3d_select(pos, mode)
            .filter(|h| h.kind.matches_mode(mode))
            .and_then(|h| h.kind.as_item())
        {
            let damage = if self.selection.contains(&item) {
                Damage::None
            } else {
                let prev = self.selection.items().to_vec();
                self.current_sector = None;
                self.selection.replace(item);
                self.selection_damage_for_mode(prev.into_iter().chain([item]).collect())
            };
            self.begin_move(world);
            return damage;
        }
        self.begin_rubber(mode, world, shift)
    }

    pub(super) fn begin_rubber(
        &mut self,
        mode: SelectMode,
        world: [f32; 2],
        shift: bool,
    ) -> Damage {
        let cleared = (!shift).then(|| {
            let items = self.selection.items().to_vec();
            let had_sector = self.current_sector.is_some();
            self.selection.clear();
            self.current_sector = None;
            (items, had_sector)
        });
        self.drag = DragState::Rubber {
            start: world,
            mode,
        };
        self.overlay = Overlay::Rubber {
            a: world,
            b: world,
        };
        match cleared {
            Some((_, true)) => Damage::Geometry,
            Some((items, false)) => self.selection_damage_for_mode(items),
            None => Damage::None,
        }
    }

    pub(super) fn rubber_select(&mut self, mode: SelectMode, a: [f32; 2], b: [f32; 2]) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let [vw, vh] = self.camera.viewport();
        let aspect = vw / vh.max(1.0);
        let proj = self.render_camera().view_proj(aspect);
        // w ≤ 0 (behind camera): skip — perspective divide would sign-flip.
        let to_ndc = |m: &Mat4, x: f32, y: f32| -> Option<[f32; 2]> {
            let v = [x, y, 0.0, 1.0];
            let mut o = [0.0f32; 4];
            for r in 0..4 {
                o[r] = (0..4).map(|c| m[c][r] * v[c]).sum();
            }
            (o[3] > 1e-6).then(|| [o[0] / o[3], o[1] / o[3]])
        };
        let corners = match (to_ndc(&proj, a[0], a[1]), to_ndc(&proj, b[0], b[1])) {
            (Some(na), Some(nb)) => Some((na, nb)),
            _ => return Damage::None,
        };
        let (lo, hi) = match corners {
            Some((na, nb)) => (
                [na[0].min(nb[0]), na[1].min(nb[1])],
                [na[0].max(nb[0]), na[1].max(nb[1])],
            ),
            None => (
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            ),
        };
        let inside = |x: f32, y: f32| {
            let Some(p) = to_ndc(&proj, x, y) else {
                return false;
            };
            p[0] >= lo[0] && p[0] <= hi[0] && p[1] >= lo[1] && p[1] <= hi[1]
        };

        let (want_verts, want_lines, want_things) = match mode {
            SelectMode::All => (true, true, true),
            SelectMode::Vertex => (true, false, false),
            SelectMode::Line => (false, true, false),
            SelectMode::Thing => (false, false, true),
            SelectMode::Sector => (false, false, false), // sectors not lasso-selectable
        };

        let mut picked = Vec::new();
        if want_verts {
            for (i, v) in map.vertices.iter().enumerate() {
                if inside(v.x, v.y) {
                    picked.push(SelItem::Vertex(i as u32));
                }
            }
        }
        if want_lines {
            for (i, line) in map.lines.iter().enumerate() {
                if let (Some(p1), Some(p2)) = (
                    map.vertices.get(line.v1 as usize),
                    map.vertices.get(line.v2 as usize),
                ) && inside(p1.x, p1.y)
                    && inside(p2.x, p2.y)
                {
                    picked.push(SelItem::Line(i as u32));
                }
            }
        }
        if want_things {
            for (i, t) in map.things.iter().enumerate() {
                if self.skill_filter.allows(t.options) && inside(t.x as f32, t.y as f32) {
                    picked.push(SelItem::Thing(i as u32));
                }
            }
        }
        for item in &picked {
            self.selection.push(*item);
        }
        self.selection_damage_for_mode(picked)
    }

    /// Indices of selected items the `pick` matcher accepts.
    pub fn selected_of(&self, pick: impl Fn(&SelItem) -> Option<u32>) -> Vec<u32> {
        self.selection.items().iter().filter_map(pick).collect()
    }

    pub fn selected_lines(&self) -> Vec<u32> {
        self.selected_of(|i| match i {
            SelItem::Line(n) => Some(*n),
            _ => None,
        })
    }

    pub fn selected_things(&self) -> Vec<u32> {
        self.selected_of(|i| match i {
            SelItem::Thing(n) => Some(*n),
            _ => None,
        })
    }

    pub fn selected_vertices(&self) -> Vec<u32> {
        self.selected_of(|i| match i {
            SelItem::Vertex(n) => Some(*n),
            _ => None,
        })
    }

    pub fn weld_selected(&mut self) -> Damage {
        let ids = self.selected_vertices();
        if ids.len() < 2 {
            return Damage::None;
        }
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::MoveSelection, map);
        let result = weld_cluster(map, &ids, VERTEX_WELD_DIST, default_sector());
        if !result.changed() {
            self.undo.discard_last();
            return Damage::None;
        }
        self.selection.clear();
        self.dirty = true;
        Damage::Geometry
    }
}
