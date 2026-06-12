//! Sector editor popup: flat previews + height spins. Apply commits via `apply_sector`.

use std::cell::RefCell;
use std::rc::Rc;

use editor_core::{Name8, Sector};
use slint::{ComponentHandle as _, VecModel};

use crate::SharedState;
use crate::boundary::FlatSlot;
use crate::generated::{EditorWindow, GfxEntry, SectorEditController};
use crate::gfx::{FLAT_SIDE, render_flat_square};
use crate::prefs::PopupWindow;
use crate::render::apply_damage;
use crate::views::view_panels as panels;
use crate::views::view_tex_browser::flat_entries;
use crate::views::view_window::restore as restore_geom;

/// Working copy of the double-clicked sector.
pub(crate) struct SectorEditDraft {
    sector_index: u32,
    sector: Sector,
    selected_slot: FlatSlot,
    entries: Vec<GfxEntry>,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_populate_sector_edit(move || {
            let Some(ui) = weak.upgrade() else { return };
            restore_geom(&ui, &s, PopupWindow::SectorEdit);
            populate(&ui, &s);
        });

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
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.sector_edit.as_mut() {
                let flat = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
                match draft.selected_slot {
                    FlatSlot::Floor => draft.sector.floor_flat = flat,
                    FlatSlot::Ceil => draft.sector.ceil_flat = flat,
                }
            }
            render(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>()
        .on_height_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<SectorEditController>();
            let (floor, ceil) = (ctl.get_floor_h(), ctl.get_ceil_h());
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.sector_edit.as_mut() {
                draft.sector.floor_height = floor;
                draft.sector.ceil_height = ceil;
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorEditController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
    });
}

/// Open edge: snapshot sector into draft, push UI.
fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow_mut().ensure_gfx() {
        log::warn!("no IWAD open - sector editor unavailable");
        ui.global::<SectorEditController>()
            .set_sector_edit_visible(false);
        return;
    }
    let ctl = ui.global::<SectorEditController>();
    let index = ctl.get_sector_index();
    let state = &mut *shared.borrow_mut();

    let Some(sector) = state
        .app
        .map
        .as_ref()
        .and_then(|m| m.sectors.get(index as usize))
        .copied()
    else {
        ctl.set_sector_edit_visible(false);
        return;
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
    ctl.set_entries(slint::ModelRc::new(VecModel::from(entries.clone())));

    state.sector_edit = Some(SectorEditDraft {
        sector_index: index as u32,
        sector,
        selected_slot: FlatSlot::Ceil,
        entries,
    });
    render(ui, state);
    scroll_to_selected(ui, state);
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
    let name = flat.to_dwd_field();
    let index = draft
        .entries
        .iter()
        .position(|e| e.name.as_str().eq_ignore_ascii_case(name));
    ctl.set_active_index(index.map_or(-1, |i| i as i32));
}

fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.sector_edit.take() else {
            return;
        };
        state.app.apply_sector(draft.sector_index, draft.sector)
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
}
