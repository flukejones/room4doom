//! Audit panel boundary: run the geometric + structural scans, list the issues, and jump the canvas to a picked one; also wires the Heal Geometry menu action.

use editor_core::validate::Issue;
use editor_core::{
    ArenaKey as _, EditorMap, GeomIssue, LineKey, SectorKey, VertKey, audit_geometry, validate,
};

use crate::app::session::{Damage, EditorSession, SelItem};
use crate::dialogs::drafts::DialogDrafts;
use crate::level_editor::HEAL_TOL;
use crate::render::view::WorldRect;

/// The map element an audit row points at.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AuditTarget {
    Line(LineKey),
    Vertex(VertKey),
    Sector(SectorKey),
}

/// Re-run both scans and push the rows; targets stay index-parallel in `EditorSession`.
pub(crate) fn refresh(drafts: &mut DialogDrafts, state: &EditorSession) {
    let mut rows: Vec<String> = Vec::new();
    let mut targets: Vec<AuditTarget> = Vec::new();
    {
        if let Some(map) = &state.map_editor.map {
            for issue in audit_geometry(map, HEAL_TOL) {
                rows.push(geom_row(&issue));
                targets.push(geom_target(&issue));
            }
            for issue in validate(map) {
                rows.push(issue.to_string());
                targets.push(AuditTarget::Line(structural_line(&issue)));
            }
        }
    }
    drafts.audit_targets = targets;
    drafts.audit.issues = rows;
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
pub(crate) fn jump_to(drafts: &DialogDrafts, state: &mut EditorSession, index: usize) -> Damage {
    let Some(target) = drafts.audit_targets.get(index).copied() else {
        return Damage::None;
    };
    {
        let Some(map) = &state.map_editor.map else {
            return Damage::None;
        };
        let Some(at) = target_point(map, target) else {
            return Damage::None;
        };
        match target {
            AuditTarget::Line(k) => state.map_editor.selection.replace(SelItem::Line(k)),
            AuditTarget::Vertex(k) => state.map_editor.selection.replace(SelItem::Vertex(k)),
            AuditTarget::Sector(k) => {
                state.map_editor.selection.replace(SelItem::Sector(k));
                state.map_editor.current_sector = Some(k);
            }
        }
        state
            .map_editor
            .camera
            .center_on(WorldRect::point(at[0], at[1]));
        Damage::Edited
    }
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
