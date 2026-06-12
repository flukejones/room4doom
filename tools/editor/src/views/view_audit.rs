//! Audit panel boundary: run the geometric + structural scans, list the issues, and jump the canvas to a picked one; also wires the Heal Geometry menu action.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, SharedString};

use editor_core::validate::Issue;
use editor_core::{
    ArenaKey as _, EditorMap, GeomIssue, LineKey, SectorKey, VertKey, audit_geometry, validate,
};

use crate::generated::{AuditController, EditorWindow};
use crate::level_editor::HEAL_TOL;
use crate::render::view::WorldRect;
use crate::state::{Damage, SelItem, SharedState};
use crate::views::model;
use crate::views::view_canvas::{after_edit, start_cam_ease};

/// The map element an audit row points at.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AuditTarget {
    Line(LineKey),
    Vertex(VertKey),
    Sector(SectorKey),
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<AuditController>().on_populate(move || {
        let Some(ui) = weak.upgrade() else { return };
        refresh(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<AuditController>().on_picked(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        jump_to(&ui, &s, index as usize);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_heal_map(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.heal_geometry();
        after_edit(&ui, &s, damage);
        if ui.global::<AuditController>().get_audit_visible() {
            refresh(&ui, &s);
        }
    });
}

/// Re-run both scans and push the rows; targets stay index-parallel in `SharedState`.
pub(crate) fn refresh(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let mut rows: Vec<SharedString> = Vec::new();
    let mut targets: Vec<AuditTarget> = Vec::new();
    {
        let state = shared.borrow();
        if let Some(map) = &state.app.map {
            for issue in audit_geometry(map, HEAL_TOL) {
                rows.push(geom_row(&issue).into());
                targets.push(geom_target(&issue));
            }
            for issue in validate(map) {
                rows.push(issue.to_string().into());
                targets.push(AuditTarget::Line(structural_line(&issue)));
            }
        }
    }
    shared.borrow_mut().audit_targets = targets;
    ui.global::<AuditController>().set_issues(model(rows));
}

fn geom_row(issue: &GeomIssue) -> String {
    match *issue {
        GeomIssue::NearCoincidentVertices {
            a,
            b,
        } => format!("vertices {} and {} nearly coincide", a.slot(), b.slot()),
        GeomIssue::UnsplitTJunction {
            line,
            vertex,
        } => format!("line {} not split at vertex {}", line.slot(), vertex.slot()),
        GeomIssue::OverlappingLines {
            a,
            b,
        } => format!("lines {} and {} overlap", a.slot(), b.slot()),
        GeomIssue::OrphanVertex(v) => format!("vertex {} is orphaned", v.slot()),
        GeomIssue::UnusedSector(s) => format!("sector {} is unused", s.slot()),
    }
}

fn geom_target(issue: &GeomIssue) -> AuditTarget {
    match *issue {
        GeomIssue::NearCoincidentVertices {
            a,
            ..
        } => AuditTarget::Vertex(a),
        GeomIssue::UnsplitTJunction {
            vertex,
            ..
        } => AuditTarget::Vertex(vertex),
        GeomIssue::OverlappingLines {
            a,
            ..
        } => AuditTarget::Line(a),
        GeomIssue::OrphanVertex(v) => AuditTarget::Vertex(v),
        GeomIssue::UnusedSector(s) => AuditTarget::Sector(s),
    }
}

fn structural_line(issue: &Issue) -> LineKey {
    match *issue {
        Issue::UnenclosedSide {
            line,
            ..
        }
        | Issue::DegenerateLine {
            line,
        }
        | Issue::TwoSidedWithoutBack {
            line,
        }
        | Issue::BackWithoutTwoSidedFlag {
            line,
        }
        | Issue::StaleRef {
            line,
        } => line,
    }
}

/// Select the picked issue's element and centre the camera on it (zoom kept).
fn jump_to(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, index: usize) {
    let Some(target) = shared.borrow().audit_targets.get(index).copied() else {
        return;
    };
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(map) = &state.app.map else { return };
        let Some(at) = target_point(map, target) else {
            return;
        };
        match target {
            AuditTarget::Line(k) => state.app.selection.replace(SelItem::Line(k)),
            AuditTarget::Vertex(k) => state.app.selection.replace(SelItem::Vertex(k)),
            AuditTarget::Sector(k) => {
                state.app.selection.replace(SelItem::Sector(k));
                state.app.current_sector = Some(k);
            }
        }
        state.app.camera.center_on(WorldRect::point(at[0], at[1]));
        Damage::Edited
    };
    after_edit(ui, shared, damage);
    start_cam_ease(ui, shared);
}

/// A representative world point for the target; `None` when the key went stale.
fn target_point(map: &EditorMap, target: AuditTarget) -> Option<[f32; 2]> {
    let line_mid = |k: LineKey| {
        let l = map.lines.get(k)?;
        let (p1, p2) = (map.vertices.get(l.v1)?, map.vertices.get(l.v2)?);
        Some([p1.x.midpoint(p2.x), p1.y.midpoint(p2.y)])
    };
    match target {
        AuditTarget::Line(k) => line_mid(k),
        AuditTarget::Vertex(k) => map.vertices.get(k).map(|v| [v.x, v.y]),
        AuditTarget::Sector(k) => {
            let line = map
                .lines
                .iter()
                .find(|(_, l)| l.sides().any(|s| s.sector == Some(k)))
                .map(|(key, _)| key)?;
            line_mid(line)
        }
    }
}
