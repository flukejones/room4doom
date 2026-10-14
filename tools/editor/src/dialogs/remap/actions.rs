//! Bulk remap dialog: scan unknown/in-use values, fill right column, Apply.

use crate::app::session::{Damage, EditorSession};
use crate::components::types::RemapKind as UiRemapKind;
use crate::components::types::RemapRow;
use crate::dialogs::drafts::DialogDrafts;
use crate::level_editor::remap::{RemapKind, RemapPair, apply_remap, collect_unknown};
use crate::undo::EditAction;
use editor_core::SpecialDef;

fn specials_for(state: &EditorSession, kind: RemapKind) -> Option<&[SpecialDef]> {
    let project = state.project.as_ref()?;
    match kind {
        RemapKind::LineSpecial => Some(&project.line_specials),
        RemapKind::SectorSpecial => Some(&project.sector_specials),
        _ => None,
    }
}

fn special_desc(specials: &[SpecialDef], value: i32) -> Option<&str> {
    specials
        .iter()
        .find(|s| s.value == value)
        .map(|s| s.desc.as_str())
}

fn rebuild_rows(drafts: &mut DialogDrafts, state: &mut EditorSession, kind: RemapKind) {
    drafts.remap.kind = kind.into();
    state.ensure_assets();
    let unknown = match &state.map_editor.map {
        Some(map) => collect_unknown(map, kind, state.assets.as_ref(), specials_for(state, kind)),
        None => Vec::new(),
    };
    drafts.remap_pairs = unknown
        .into_iter()
        .map(|from| RemapPair {
            from,
            to: String::new(),
        })
        .collect();
    // Special kinds: label shows description; pair stores numeric value for Apply.
    let rows: Vec<RemapRow> = {
        let specials = specials_for(state, kind);
        drafts
            .remap_pairs
            .iter()
            .map(|p| {
                let desc = specials.and_then(|sp| {
                    let value = p.from.parse::<i32>().ok()?;
                    special_desc(sp, value)
                });
                let label = desc.map(str::to_owned).unwrap_or_else(|| p.from.clone());
                RemapRow {
                    from: label,
                    to: p.to.clone(),
                }
            })
            .collect()
    };
    drafts.remap.rows = rows;
}

pub(crate) fn populate(drafts: &mut DialogDrafts, state: &mut EditorSession) {
    rebuild_rows(drafts, state, RemapKind::Thing);
    drafts.remap.status.clear();
}

pub(crate) fn kind_changed(
    drafts: &mut DialogDrafts,
    state: &mut EditorSession,
    kind: UiRemapKind,
) {
    rebuild_rows(drafts, state, kind.into());
}

pub(crate) fn to_edited(
    drafts: &mut DialogDrafts,
    state: &EditorSession,
    index: i32,
    text: String,
) {
    let kind = RemapKind::from(drafts.remap.kind);
    if let Some(row) = drafts.remap.rows.get_mut(index as usize) {
        row.to.clone_from(&text);
    }
    // Special-kind: description resolves to numeric value; otherwise pass through.
    let resolved = specials_for(state, kind)
        .and_then(|sp| {
            sp.iter()
                .find(|d| d.desc.eq_ignore_ascii_case(text.trim()))
                .map(|d| d.value.to_string())
        })
        .unwrap_or(text);
    if let Some(pair) = drafts.remap_pairs.get_mut(index as usize) {
        pair.to = resolved;
    }
}

pub(crate) fn apply(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    let (damage, changed, invalid, kind) = {
        let kind = RemapKind::from(drafts.remap.kind);
        let filled: Vec<RemapPair> = drafts
            .remap_pairs
            .iter()
            .filter(|p| !p.to.trim().is_empty())
            .cloned()
            .collect();
        let invalid = filled
            .iter()
            .filter(|p| p.to.trim().parse::<i32>().is_err())
            .count();
        let pairs: Vec<RemapPair> = filled
            .into_iter()
            .filter(|p| p.to.trim().parse::<i32>().is_ok())
            .collect();
        let Some(map) = &mut state.map_editor.map else {
            return Damage::None;
        };
        if pairs.is_empty() {
            (Damage::None, 0, invalid, kind)
        } else {
            state.map_editor.undo.record(EditAction::RemapApply, map);
            let changed = apply_remap(map, kind, &pairs);
            if changed > 0 {
                state.map_editor.dirty = true;
                (Damage::Edited, changed, invalid, kind)
            } else {
                state.map_editor.undo.discard_last();
                (Damage::None, 0, invalid, kind)
            }
        }
    };
    let status = if invalid > 0 {
        format!("changed {changed} fields, {invalid} skipped (not a number)")
    } else {
        format!("changed {changed} fields")
    };
    drafts.remap.status = status;
    rebuild_rows(drafts, state, kind);
    damage
}

impl From<RemapKind> for UiRemapKind {
    fn from(k: RemapKind) -> Self {
        match k {
            RemapKind::Thing => Self::Thing,
            RemapKind::Texture => Self::Texture,
            RemapKind::Flat => Self::Flat,
            RemapKind::LineSpecial => Self::LineSpecial,
            RemapKind::SectorSpecial => Self::SectorSpecial,
        }
    }
}

impl From<UiRemapKind> for RemapKind {
    fn from(k: UiRemapKind) -> Self {
        match k {
            UiRemapKind::Thing => Self::Thing,
            UiRemapKind::Texture => Self::Texture,
            UiRemapKind::Flat => Self::Flat,
            UiRemapKind::LineSpecial => Self::LineSpecial,
            UiRemapKind::SectorSpecial => Self::SectorSpecial,
        }
    }
}
