//! Selection: click-select, rubber-band lasso, and the multi-sector selection.

use editor_core::{LineKey, SectorKey, ThingKey, VertKey, weld_cluster};

use super::{LevelEditorState, VERTEX_WELD_DIST};
use crate::boundary::SelectMode;
use crate::level_editor::draw::default_sector;
use crate::level_editor::pick3d::PickKind;
use crate::state::{Damage, DragState, Overlay, SelItem};
use crate::undo::EditAction;

impl LevelEditorState {
    pub fn clear_selection(&mut self) -> Damage {
        let had_any = !self.selection.is_empty() || self.current_sector.is_some();
        self.selection.clear();
        self.current_sector = None;
        // No selection → grid resets to z=0.
        let plane_reset = self.set_grid_plane(0.0);
        if had_any {
            Damage::Edited
        } else if plane_reset {
            Damage::View
        } else {
            Damage::None
        }
    }

    pub(super) fn select_pick(&mut self, kind: PickKind, shift: bool) -> Damage {
        let prev_sector = self.current_sector;
        let prev_selection = self.selection.clone();
        match kind {
            PickKind::Vertex(k) => self.toggle_or_replace(SelItem::Vertex(k), shift),
            PickKind::Linedef(k) => self.toggle_or_replace(SelItem::Line(k), shift),
            PickKind::Thing(k) => self.toggle_or_replace(SelItem::Thing(k), shift),
            PickKind::Sector(s) => {
                self.current_sector = if shift && Some(s) == self.current_sector {
                    None
                } else {
                    Some(s)
                };
                self.update_selected_sectors(Some(s), shift);
            }
        }
        if self.current_sector != prev_sector || self.selection != prev_selection {
            Damage::Edited
        } else {
            Damage::None
        }
    }

    fn toggle_or_replace(&mut self, item: SelItem, shift: bool) {
        self.current_sector = None;
        if shift {
            self.selection.toggle(item);
        } else {
            self.selection.replace(item);
        }
    }

    fn update_selected_sectors(&mut self, under: Option<SectorKey>, shift: bool) {
        // A plain click replaces the whole selection, matching every other element kind.
        if !shift {
            self.selection.clear();
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
        if let Some(hit) = self
            .pick_3d_select(pos, mode)
            .filter(|h| h.kind.matches_mode(mode))
            && let Some(item) = hit.kind.as_item()
        {
            // Snap the editing plane to the picked element, THEN anchor the drag — the anchor must be unprojected on the same plane the drag will use.
            let plane_moved = self.set_grid_plane(hit.grid_z);
            let world = if plane_moved {
                self.screen_to_world(pos)
            } else {
                world
            };
            self.cursor_world = world;
            let damage = if self.selection.contains(&item) {
                Damage::None
            } else {
                self.current_sector = None;
                self.selection.replace(item);
                Damage::Edited
            };
            self.begin_move(world);
            return if plane_moved {
                Damage::View.combine(damage)
            } else {
                damage
            };
        }
        self.begin_rubber(mode, world, shift)
    }

    pub(super) fn begin_rubber(
        &mut self,
        mode: SelectMode,
        world: [f32; 2],
        shift: bool,
    ) -> Damage {
        let mut cleared = false;
        if !shift {
            cleared = !self.selection.is_empty() || self.current_sector.is_some();
            self.selection.clear();
            self.current_sector = None;
        }
        self.drag = DragState::Rubber {
            start: world,
            mode,
        };
        self.overlay = Overlay::Rubber {
            a: world,
            b: world,
        };
        if cleared {
            Damage::Edited
        } else {
            Damage::None
        }
    }

    pub(super) fn rubber_select(&mut self, mode: SelectMode, a: [f32; 2], b: [f32; 2]) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let [vw, vh] = self.camera.viewport();
        let aspect = vw / vh.max(1.0);
        let cam = self.render_camera();
        // Behind-camera points project to None (perspective divide would sign-flip).
        let to_ndc = |x: f32, y: f32| cam.world_to_ndc([x, y, 0.0], aspect);
        let (Some(na), Some(nb)) = (to_ndc(a[0], a[1]), to_ndc(b[0], b[1])) else {
            return Damage::None;
        };
        let lo = [na[0].min(nb[0]), na[1].min(nb[1])];
        let hi = [na[0].max(nb[0]), na[1].max(nb[1])];
        let inside = |x: f32, y: f32| {
            let Some(p) = to_ndc(x, y) else {
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
            for (k, v) in map.vertices.iter() {
                if inside(v.x, v.y) {
                    picked.push(SelItem::Vertex(k));
                }
            }
        }
        if want_lines {
            for (k, line) in map.lines.iter() {
                if let (Some(p1), Some(p2)) = (map.vertices.get(line.v1), map.vertices.get(line.v2))
                    && inside(p1.x, p1.y)
                    && inside(p2.x, p2.y)
                {
                    picked.push(SelItem::Line(k));
                }
            }
        }
        if want_things {
            for (k, t) in map.things.iter() {
                if self.skill_filter.allows(t.options) && inside(t.x as f32, t.y as f32) {
                    picked.push(SelItem::Thing(k));
                }
            }
        }
        if picked.is_empty() {
            return Damage::None;
        }
        for item in &picked {
            self.selection.push(*item);
        }
        Damage::Edited
    }

    /// Keys of selected items the `pick` matcher accepts.
    pub fn selected_of<T>(&self, pick: impl Fn(&SelItem) -> Option<T>) -> Vec<T> {
        self.selection.items().iter().filter_map(pick).collect()
    }

    pub fn selected_lines(&self) -> Vec<LineKey> {
        self.selected_of(|i| match i {
            SelItem::Line(k) => Some(*k),
            _ => None,
        })
    }

    pub fn selected_things(&self) -> Vec<ThingKey> {
        self.selected_of(|i| match i {
            SelItem::Thing(k) => Some(*k),
            _ => None,
        })
    }

    pub fn selected_vertices(&self) -> Vec<VertKey> {
        self.selected_of(|i| match i {
            SelItem::Vertex(k) => Some(*k),
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
        if !weld_cluster(map, &ids, VERTEX_WELD_DIST, default_sector()) {
            self.undo.discard_last();
            return Damage::None;
        }
        self.selection.clear();
        self.dirty = true;
        Damage::Edited
    }
}
