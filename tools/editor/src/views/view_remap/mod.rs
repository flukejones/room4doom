//! Bulk remap dialog: scan unknown/in-use values, fill right column, Apply.

mod conversions;

use std::cell::RefCell;
use std::rc::Rc;

use editor_core::SpecialDef;
use slint::ComponentHandle as _;

use crate::generated::{EditorWindow, RemapController, RemapRow};
use crate::level_editor::remap::{RemapKind, RemapPair, apply_remap, collect_unknown};
use crate::prefs::PopupWindow;
use crate::render::apply_damage;
use crate::state::{Damage, SharedState};
use crate::undo::EditAction;
use crate::views::model;
use crate::views::view_window::restore as restore_geom;

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<RemapController>().on_populate_remap(move || {
        let Some(ui) = weak.upgrade() else { return };
        restore_geom(&ui, &s, PopupWindow::Remap);
        rebuild_rows(&ui, &s, RemapKind::Thing);
        let ctl = ui.global::<RemapController>();
        ctl.set_kind(RemapKind::Thing.into());
        ctl.set_status("".into());
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<RemapController>().on_kind_changed(move |kind| {
        let Some(ui) = weak.upgrade() else { return };
        let kind = RemapKind::from(kind);
        ui.global::<RemapController>().set_kind(kind.into());
        rebuild_rows(&ui, &s, kind);
    });

    let s = shared.clone();
    ui.global::<RemapController>()
        .on_to_edited(move |index, text| {
            let state = &mut *s.borrow_mut();
            let kind = state.remap_kind;
            // Special-kind: description resolves to numeric value; otherwise pass through.
            let resolved = specials_for(state, kind)
                .and_then(|sp| {
                    sp.iter()
                        .find(|d| d.desc.eq_ignore_ascii_case(text.trim()))
                        .map(|d| d.value.to_string())
                })
                .unwrap_or_else(|| text.to_string());
            if let Some(pair) = state.remap_pairs.get_mut(index as usize) {
                pair.to = resolved;
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<RemapController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        let (damage, changed, invalid, kind) = {
            let state = &mut *s.borrow_mut();
            let kind = state.remap_kind;
            let filled: Vec<RemapPair> = state
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
            let Some(map) = &mut state.app.map else {
                return;
            };
            if pairs.is_empty() {
                (Damage::None, 0, invalid, kind)
            } else {
                state.app.undo.record(EditAction::RemapApply, map);
                let changed = apply_remap(map, kind, &pairs);
                if changed > 0 {
                    state.app.dirty = true;
                    (Damage::Edited, changed, invalid, kind)
                } else {
                    state.app.undo.discard_last();
                    (Damage::None, 0, invalid, kind)
                }
            }
        };
        let status = if invalid > 0 {
            slint::format!("changed {changed} fields, {invalid} skipped (not a number)")
        } else {
            slint::format!("changed {changed} fields")
        };
        ui.global::<RemapController>().set_status(status);
        apply_damage(&ui, &s, damage);
        rebuild_rows(&ui, &s, kind);
    });
}

fn specials_for(state: &SharedState, kind: RemapKind) -> Option<&[SpecialDef]> {
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

fn rebuild_rows(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, kind: RemapKind) {
    let state = &mut *shared.borrow_mut();
    state.remap_kind = kind;
    state.ensure_assets();
    let unknown = match &state.app.map {
        Some(map) => collect_unknown(map, kind, state.assets.as_ref(), specials_for(state, kind)),
        None => Vec::new(),
    };
    state.remap_pairs = unknown
        .into_iter()
        .map(|from| RemapPair {
            from,
            to: String::new(),
        })
        .collect();
    // Special kinds: label shows description; pair stores numeric value for Apply.
    let rows: Vec<RemapRow> = {
        let specials = specials_for(state, kind);
        state
            .remap_pairs
            .iter()
            .map(|p| {
                let desc = specials.and_then(|sp| {
                    let value = p.from.parse::<i32>().ok()?;
                    special_desc(sp, value)
                });
                let label = desc.map(str::to_owned).unwrap_or_else(|| p.from.clone());
                RemapRow {
                    from: label.into(),
                    to: p.to.clone().into(),
                }
            })
            .collect()
    };
    ui.global::<RemapController>().set_rows(model(rows));
}
