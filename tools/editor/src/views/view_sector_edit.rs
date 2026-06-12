//! Sector editor popup: edits write through live; Apply = one undo step, close = revert.

use std::cell::RefCell;
use std::rc::Rc;

use editor_core::{Name8, Sector, SectorKey};
use slint::{ComponentHandle as _, Model as _};

use crate::SharedState;
use crate::assets::FLAT_SIDE;
use crate::boundary::FlatSlot;
use crate::generated::{EditorWindow, SectorEditController};
use crate::gfx::render_flat_square;
use crate::prefs::PopupWindow;
use crate::render::apply_damage;
use crate::undo::{EditAction, EditSession};
use crate::views::model;
use crate::views::view_panels as panels;
use crate::views::view_tex_browser::{entry_index, flat_entries};
use crate::views::view_window::restore as restore_geom;

/// Session for the double-clicked sector; the flat list lives in the Slint model.
pub(crate) struct SectorEditDraft {
    sector_key: SectorKey,
    sector: Sector,
    selected_slot: FlatSlot,
    session: EditSession,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_select_flat(move |slot| {
            let Some(ui) = weak.upgrade() else { return };
            let slot = FlatSlot::from(slot);
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.sector_edit.as_mut() {
                draft.selected_slot = slot;
            }
            ui.global::<SectorEditController>()
                .set_selected_slot(slot.into());
            scroll_to_selected(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_pick_flat(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let state = &mut *s.borrow_mut();
                if let Some(draft) = state.sector_edit.as_mut() {
                    let flat = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
                    match draft.selected_slot {
                        FlatSlot::Floor => draft.sector.floor_flat = flat,
                        FlatSlot::Ceil => draft.sector.ceil_flat = flat,
                    }
                }
            }
            write_through(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_height_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<SectorEditController>();
            let (floor, ceil) = (ctl.get_floor_h(), ctl.get_ceil_h());
            {
                let state = &mut *s.borrow_mut();
                if let Some(draft) = state.sector_edit.as_mut() {
                    draft.sector.floor_height = floor;
                    draft.sector.ceil_height = ceil;
                }
            }
            write_through(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_sector_edit_closed(move || {
            let Some(ui) = weak.upgrade() else { return };
            close_live_draft(&ui, &s);
        });
}

/// Double-click open: cancel any live draft, rebuild for `sector_slot`; false when unavailable.
pub(crate) fn open(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, sector_slot: i32) -> bool {
    close_live_draft(ui, shared);
    ui.global::<SectorEditController>()
        .set_sector_index(sector_slot);
    restore_geom(ui, shared, PopupWindow::SectorEdit);
    populate(ui, shared)
}

/// Revert and drop the live draft (shared by close and reopen).
fn close_live_draft(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.sector_edit.take() else {
            return;
        };
        state.app.cancel_session(draft.session)
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
}

/// Write the draft sector to the map (no undo record); refresh canvas, panels, preview.
fn write_through(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.sector_edit.as_ref() else {
            return;
        };
        let (key, sector) = (draft.sector_key, draft.sector);
        state.app.set_sector(key, sector)
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
    render(ui, &shared.borrow());
}

/// Snapshot the sector into a draft and push the UI; false when unavailable.
fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) -> bool {
    if !shared.borrow_mut().ensure_gfx() {
        log::warn!("no IWAD open - sector editor unavailable");
        return false;
    }
    let ctl = ui.global::<SectorEditController>();
    let index = ctl.get_sector_index();
    let state = &mut *shared.borrow_mut();

    let Some((key, sector)) = state.app.map.as_ref().and_then(|m| {
        let key = m.sectors.key_at_slot(index as u32)?;
        Some((key, *m.sectors.get(key)?))
    }) else {
        return false;
    };
    let Some(session) = state.app.begin_session() else {
        return false;
    };

    let entries = {
        let SharedState {
            gfx,
            assets,
            ..
        } = &mut *state;
        let gfx = gfx.as_mut().expect("ensured above");
        let assets = assets.as_ref().expect("ensured above");
        flat_entries(gfx, assets, "")
    };
    ctl.set_floor_h(sector.floor_height);
    ctl.set_ceil_h(sector.ceil_height);
    ctl.set_selected_slot(FlatSlot::Ceil.into());
    ctl.set_entries(model(entries));

    state.sector_edit = Some(SectorEditDraft {
        sector_key: key,
        sector,
        selected_slot: FlatSlot::Ceil,
        session,
    });
    render(ui, state);
    scroll_to_selected(ui, state);
    true
}

fn render(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<SectorEditController>();
    let Some(draft) = state.sector_edit.as_ref() else {
        return;
    };
    let pixel_ratio = ui.window().scale_factor();
    let physical = (FLAT_SIDE as f32 * pixel_ratio).ceil() as u32;
    let assets = state.assets.as_ref().expect("populate ensured assets");
    ctl.set_ceil_img(render_flat_square(assets, draft.sector.ceil_flat, physical));
    ctl.set_floor_img(render_flat_square(
        assets,
        draft.sector.floor_flat,
        physical,
    ));
}

/// Mark selected slot's flat as active list row (Slint highlights + scrolls).
fn scroll_to_selected(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<SectorEditController>();
    let Some(draft) = state.sector_edit.as_ref() else {
        return;
    };
    let flat = match draft.selected_slot {
        FlatSlot::Floor => draft.sector.floor_flat,
        FlatSlot::Ceil => draft.sector.ceil_flat,
    };
    ctl.set_active_index(entry_index(
        ctl.get_entries().iter().map(|e| e.name),
        flat.to_dwd_field(),
    ));
}

/// Commit the session: map already holds the live edits; record them as one undo step.
fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    write_through(ui, shared);
    let state = &mut *shared.borrow_mut();
    if let Some(draft) = state.sector_edit.take() {
        state
            .app
            .commit_session(EditAction::EditSector, draft.session);
    }
}
