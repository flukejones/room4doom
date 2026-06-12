//! Wall editor popup: sidedef elevation, band picker, texture list, offset nudge.
//! Apply commits via `apply_line`/`apply_sector`.

use std::cell::RefCell;
use std::rc::Rc;

use super::{KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, NUDGE_STEP, NUDGE_STEP_SHIFT};
use editor_core::geom::line_length;
use editor_core::{EditorMap, LineDef, Name8, Sector, SideDef};
use slint::{ComponentHandle as _, VecModel};

use crate::SharedState;
use crate::boundary::TexSlot;
use crate::generated::{BandRect, EditorWindow, GfxEntry, WallEditController};
use crate::gfx::{WallBand, render_wall};
use crate::level_editor::preview::side_bands_tagged;
use crate::prefs::PopupWindow;
use crate::render::apply_damage;
use crate::state::Damage;
use crate::views::view_panels as panels;
use crate::views::view_tex_browser::texture_entries;
use crate::views::view_window::restore as restore_geom;

/// Texel→px scale for the wall elevation.
const WALL_PX_PER_UNIT: f32 = 2.0;
/// Fit-box (logical px) for the wall elevation.
const WALL_FIT_W: f32 = 360.0;
const WALL_FIT_H: f32 = 460.0;

/// Working copy of the double-clicked line; commits on Apply.
pub(crate) struct WallEditDraft {
    line_index: u32,
    front: SideDef,
    back: Option<SideDef>,
    /// Front sector — height spins edit its floor/ceil.
    front_sector: u32,
    floor_h: i32,
    ceil_h: i32,
    selected_slot: TexSlot,
    /// Cached texture list shared by all bands.
    textures: Vec<GfxEntry>,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_populate_wall_edit(move || {
            let Some(ui) = weak.upgrade() else { return };
            restore_geom(&ui, &s, PopupWindow::WallEdit);
            populate(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>().on_side_changed(move || {
        let Some(ui) = weak.upgrade() else { return };
        let state = &mut *s.borrow_mut();
        let slot = default_slot(&ui, state);
        if let Some(draft) = state.wall_edit.as_mut() {
            draft.selected_slot = slot;
        }
        render(&ui, state);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_select_band(move |slot| {
            let Some(ui) = weak.upgrade() else { return };
            let slot = TexSlot::from(slot);
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.wall_edit.as_mut() {
                draft.selected_slot = slot;
            }
            ui.global::<WallEditController>()
                .set_selected_slot(slot.into());
            scroll_to_selected(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_pick_texture(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.wall_edit.as_mut() {
                let tex = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
                set_slot_tex(draft, tex);
            }
            render(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_height_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<WallEditController>();
            let (floor, ceil) = (ctl.get_floor_h(), ctl.get_ceil_h());
            let state = &mut *s.borrow_mut();
            if let Some(draft) = state.wall_edit.as_mut() {
                draft.floor_h = floor;
                draft.ceil_h = ceil;
            }
            render(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_wall_edit_key(move |key, shift| {
            let Some(ui) = weak.upgrade() else { return };
            let state = &mut *s.borrow_mut();
            nudge(state, &key, shift);
            push_offsets(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
    });
}

/// Open edge: snapshot line into draft, push UI.
fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow_mut().ensure_gfx() {
        log::warn!("no IWAD open - wall editor unavailable");
        ui.global::<WallEditController>()
            .set_wall_edit_visible(false);
        return;
    }
    let ctl = ui.global::<WallEditController>();
    let index = ctl.get_line_index();
    let state = &mut *shared.borrow_mut();

    let Some((front, back, front_sector)) = state.app.map.as_ref().and_then(|m| {
        let line = m.lines.get(index as usize)?;
        Some((line.front, line.back, line.front.sector?))
    }) else {
        ctl.set_wall_edit_visible(false);
        return;
    };
    let Some(sector) = state
        .app
        .map
        .as_ref()
        .and_then(|m| m.sectors.get(front_sector as usize))
        .copied()
    else {
        ctl.set_wall_edit_visible(false);
        return;
    };

    let selected_slot = default_slot_for(state, index as u32, 0);
    let textures = {
        let SharedState {
            gfx,
            assets,
            wad_data,
            ..
        } = &mut *state;
        let gfx = gfx.as_mut().expect("ensured above");
        let assets = assets.as_ref().expect("ensured above");
        let wad = wad_data.as_ref().expect("ensured above");
        texture_entries(gfx, assets, wad, "")
    };
    ctl.set_has_back(back.is_some());
    ctl.set_side_tab(0);
    ctl.set_floor_h(sector.floor_height);
    ctl.set_ceil_h(sector.ceil_height);
    ctl.set_textures(slint::ModelRc::new(VecModel::from(textures.clone())));
    ctl.set_selected_slot(selected_slot.into());

    state.wall_edit = Some(WallEditDraft {
        line_index: index as u32,
        front,
        back,
        front_sector,
        floor_h: sector.floor_height,
        ceil_h: sector.ceil_height,
        selected_slot,
        textures,
    });
    render(ui, state);
    scroll_to_selected(ui, state);
}

/// Render active side elevation; push image, band rects, offsets.
fn render(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<WallEditController>();
    let Some(draft) = state.wall_edit.as_ref() else {
        return;
    };
    let side_tab = ctl.get_side_tab();
    let is_front = side_tab == 0;
    let Some(map) = state.app.map.as_ref() else {
        return;
    };
    let Some(line) = map.lines.get(draft.line_index as usize) else {
        return;
    };
    let side = if is_front {
        draft.front
    } else {
        match draft.back {
            Some(b) => b,
            None => return,
        }
    };

    // Front sector carries unsaved height edits.
    let bands = draft_bands(map, draft, &side, is_front);
    let width_world = line_length(map, line).unwrap_or(0.0);
    let total: f32 = bands.iter().map(|(_, b)| b.height).sum();
    if width_world <= 0.0 || total <= 0.0 {
        ctl.set_wall_w(0.0);
        ctl.set_wall_h(0.0);
        ctl.set_bands(slint::ModelRc::new(VecModel::from(Vec::<BandRect>::new())));
        push_offsets(ui, state);
        return;
    }

    let px = (WALL_FIT_W / width_world)
        .min(WALL_FIT_H / total)
        .min(WALL_PX_PER_UNIT);
    let plain: Vec<WallBand> = bands
        .iter()
        .map(|(_, b)| WallBand {
            tex: b.tex,
            height: b.height,
            masked: b.masked,
        })
        .collect();
    let pixel_ratio = ui.window().scale_factor();
    let assets = state.assets.as_ref().expect("populate ensured assets");
    let image = render_wall(assets, &plain, width_world, px * pixel_ratio);

    let rects = band_rects(&bands, px);
    if let Some(image) = image {
        ctl.set_wall_img(image);
    }
    ctl.set_wall_w(width_world * px);
    ctl.set_wall_h(total * px);
    ctl.set_bands(slint::ModelRc::new(VecModel::from(rects)));
    push_offsets(ui, state);
}

/// Band hit-rects in logical px, top to bottom.
fn band_rects(bands: &[(TexSlot, WallBand)], px: f32) -> Vec<BandRect> {
    let mut rects = Vec::with_capacity(bands.len());
    let mut y = 0.0f32;
    for (slot, band) in bands {
        let h = band.height * px;
        rects.push(BandRect {
            y,
            h,
            slot: (*slot).into(),
        });
        y += h;
    }
    rects
}

fn push_offsets(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<WallEditController>();
    let Some(draft) = state.wall_edit.as_ref() else {
        return;
    };
    let side = active_side(ui, draft);
    ctl.set_xoff(side.x_offset);
    ctl.set_yoff(side.y_offset);
}

/// Default slot for active side: topmost present band.
fn default_slot(ui: &EditorWindow, state: &SharedState) -> TexSlot {
    let side_tab = ui.global::<WallEditController>().get_side_tab();
    state
        .wall_edit
        .as_ref()
        .map(|d| default_slot_for(state, d.line_index, side_tab))
        .unwrap_or(TexSlot::FrontMid)
}

fn default_slot_for(state: &SharedState, line_index: u32, side_tab: i32) -> TexSlot {
    let is_front = side_tab == 0;
    let bands = state
        .app
        .map
        .as_ref()
        .and_then(|m| {
            let line = m.lines.get(line_index as usize)?;
            let side = if is_front { line.front } else { line.back? };
            let own = m.sectors.get(side.sector? as usize)?;
            let other_idx = if is_front {
                line.back.and_then(|b| b.sector)
            } else {
                line.front.sector
            };
            let other = other_idx.and_then(|i| m.sectors.get(i as usize));
            Some(side_bands_tagged(own, other, &side, is_front))
        })
        .unwrap_or_default();
    bands.first().map(|(slot, _)| *slot).unwrap_or(if is_front {
        TexSlot::FrontMid
    } else {
        TexSlot::BackMid
    })
}

/// Mark selected band's texture as active list row (Slint highlights + scrolls).
fn scroll_to_selected(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<WallEditController>();
    let Some(draft) = state.wall_edit.as_ref() else {
        return;
    };
    let tex = slot_tex(draft);
    let name = tex.to_dwd_field();
    let index = draft
        .textures
        .iter()
        .position(|e| e.name.as_str().eq_ignore_ascii_case(name));
    ctl.set_active_index(index.map_or(-1, |i| i as i32));
}

fn active_side(ui: &EditorWindow, draft: &WallEditDraft) -> SideDef {
    if ui.global::<WallEditController>().get_side_tab() == 0 {
        draft.front
    } else {
        draft.back.unwrap_or(draft.front)
    }
}

fn slot_tex(draft: &WallEditDraft) -> Name8 {
    let side = match draft.selected_slot {
        TexSlot::FrontTop | TexSlot::FrontMid | TexSlot::FrontBottom => draft.front,
        _ => draft.back.unwrap_or(draft.front),
    };
    match draft.selected_slot {
        TexSlot::FrontTop | TexSlot::BackTop => side.top_tex,
        TexSlot::FrontMid | TexSlot::BackMid => side.middle_tex,
        TexSlot::FrontBottom | TexSlot::BackBottom => side.bottom_tex,
    }
}

fn set_slot_tex(draft: &mut WallEditDraft, tex: Name8) {
    let slot = draft.selected_slot;
    let side = match slot {
        TexSlot::FrontTop | TexSlot::FrontMid | TexSlot::FrontBottom => &mut draft.front,
        _ => match draft.back.as_mut() {
            Some(b) => b,
            None => return,
        },
    };
    match slot {
        TexSlot::FrontTop | TexSlot::BackTop => side.top_tex = tex,
        TexSlot::FrontMid | TexSlot::BackMid => side.middle_tex = tex,
        TexSlot::FrontBottom | TexSlot::BackBottom => side.bottom_tex = tex,
    }
}

fn nudge(state: &mut SharedState, key: &str, shift: bool) {
    let step = if shift { NUDGE_STEP_SHIFT } else { NUDGE_STEP };
    let (dx, dy) = match key {
        KEY_LEFT => (-step, 0),
        KEY_RIGHT => (step, 0),
        KEY_UP => (0, -step),
        KEY_DOWN => (0, step),
        _ => return,
    };
    let Some(draft) = state.wall_edit.as_mut() else {
        return;
    };
    // UI tab not reachable here; infer side from slot family.
    let front = matches!(
        draft.selected_slot,
        TexSlot::FrontTop | TexSlot::FrontMid | TexSlot::FrontBottom
    );
    let side = if front {
        &mut draft.front
    } else {
        match draft.back.as_mut() {
            Some(b) => b,
            None => return,
        }
    };
    side.x_offset += dx;
    side.y_offset += dy;
}

/// Commit draft: write line + (if changed) front sector.
fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.wall_edit.take() else {
            return;
        };
        let Some((v1, v2, flags, special, tag)) = state
            .app
            .map
            .as_ref()
            .and_then(|m| m.lines.get(draft.line_index as usize))
            .map(|l| (l.v1, l.v2, l.flags, l.special, l.tag))
        else {
            return;
        };
        let new_line = LineDef {
            v1,
            v2,
            flags,
            special,
            tag,
            front: draft.front,
            back: draft.back,
        };
        let line_dmg = state.app.apply_line(draft.line_index, new_line);

        let sector_dmg = state
            .app
            .map
            .as_ref()
            .and_then(|m| m.sectors.get(draft.front_sector as usize))
            .copied()
            .map(|old| {
                let new = Sector {
                    floor_height: draft.floor_h,
                    ceil_height: draft.ceil_h,
                    ..old
                };
                state.app.apply_sector(draft.front_sector, new)
            })
            .unwrap_or(Damage::None);

        line_dmg.combine(sector_dmg)
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
}

/// Bands for active side with draft height edits applied to the front sector.
fn draft_bands(
    map: &EditorMap,
    draft: &WallEditDraft,
    side: &SideDef,
    is_front: bool,
) -> Vec<(TexSlot, WallBand)> {
    let other_sector_idx = if is_front {
        draft.back.and_then(|b| b.sector)
    } else {
        draft.front.sector
    };
    let own = match side.sector.and_then(|i| map.sectors.get(i as usize)) {
        Some(s) => with_draft_heights(*s, side.sector.expect("checked above"), draft),
        None => return Vec::new(),
    };
    let other = other_sector_idx
        .and_then(|i| map.sectors.get(i as usize).copied())
        .map(|s| with_draft_heights(s, other_sector_idx.expect("checked above"), draft));
    side_bands_tagged(&own, other.as_ref(), side, is_front)
}

fn with_draft_heights(mut sector: Sector, index: u32, draft: &WallEditDraft) -> Sector {
    if index == draft.front_sector {
        sector.floor_height = draft.floor_h;
        sector.ceil_height = draft.ceil_h;
    }
    sector
}
