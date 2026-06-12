//! Wall editor popup: edits write through live; Apply = one undo step, close = revert.

use std::cell::RefCell;
use std::rc::Rc;

use super::{KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, NUDGE_STEP, NUDGE_STEP_SHIFT};
use editor_core::geom::line_length;
use editor_core::{EditorMap, LineDef, LineKey, Name8, Sector, SectorKey, SideDef};
use slint::{ComponentHandle as _, Model as _};

use crate::SharedState;
use crate::boundary::TexSlot;
use crate::generated::{BandRect, EditorWindow, WallEditController};
use crate::gfx::{WallBand, render_wall};
use crate::level_editor::preview::side_bands_tagged;
use crate::prefs::PopupWindow;
use crate::render::apply_damage;
use crate::state::Damage;
use crate::undo::{EditAction, EditSession};
use crate::views::model;
use crate::views::view_panels as panels;
use crate::views::view_tex_browser::{entry_index, texture_entries};
use crate::views::view_window::restore as restore_geom;

/// Texel→px scale for the wall elevation.
const WALL_PX_PER_UNIT: f32 = 2.0;
/// Fit-box (logical px) for the wall elevation.
const WALL_FIT_W: f32 = 360.0;
const WALL_FIT_H: f32 = 460.0;

/// One side's sector heights; the spin row edits the active tab's copy.
#[derive(Clone, Copy)]
struct SideHeights {
    sector: SectorKey,
    floor_h: i32,
    ceil_h: i32,
}

/// Edit session for the double-clicked line: edits write through live; Apply records one undo step, close-without-Apply reverts.
pub(crate) struct WallEditDraft {
    line: LineKey,
    front: SideDef,
    back: Option<SideDef>,
    front_heights: SideHeights,
    back_heights: Option<SideHeights>,
    selected_slot: TexSlot,
    session: EditSession,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
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
        push_heights(&ui, state);
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
            push_offsets(&ui, state);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_pick_texture(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let state = &mut *s.borrow_mut();
                if let Some(draft) = state.wall_edit.as_mut() {
                    let tex = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
                    set_slot_tex(draft, tex);
                }
            }
            write_through(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_height_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<WallEditController>();
            let (floor, ceil) = (ctl.get_floor_h(), ctl.get_ceil_h());
            let is_front = ctl.get_side_tab() == 0;
            {
                let state = &mut *s.borrow_mut();
                let heights = state.wall_edit.as_mut().and_then(|draft| {
                    if is_front {
                        Some(&mut draft.front_heights)
                    } else {
                        draft.back_heights.as_mut()
                    }
                });
                if let Some(h) = heights {
                    h.floor_h = floor;
                    h.ceil_h = ceil;
                }
            }
            write_through(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_wall_edit_key(move |key, shift| {
            let Some(ui) = weak.upgrade() else { return };
            nudge(&mut s.borrow_mut(), &key, shift);
            write_through(&ui, &s);
            push_offsets(&ui, &s.borrow());
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_offset_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<WallEditController>();
            let (x, y) = (ctl.get_xoff(), ctl.get_yoff());
            {
                let state = &mut *s.borrow_mut();
                if let Some(side) = state.wall_edit.as_mut().and_then(offset_side_mut) {
                    side.x_offset = x;
                    side.y_offset = y;
                }
            }
            write_through(&ui, &s);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<WallEditController>()
        .on_wall_edit_closed(move || {
            let Some(ui) = weak.upgrade() else { return };
            close_live_draft(&ui, &s);
        });
}

/// Double-click open: cancel any live draft, rebuild for `line_slot`; false when unavailable.
pub(crate) fn open(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, line_slot: i32) -> bool {
    close_live_draft(ui, shared);
    ui.global::<WallEditController>().set_line_index(line_slot);
    restore_geom(ui, shared, PopupWindow::WallEdit);
    populate(ui, shared)
}

/// Revert and drop the live draft (shared by close and reopen).
fn close_live_draft(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.wall_edit.take() else {
            return;
        };
        state.app.cancel_session(draft.session)
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
}

/// Snapshot the line into a draft and push the UI; false when unavailable.
fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) -> bool {
    if !shared.borrow_mut().ensure_gfx() {
        log::warn!("no IWAD open - wall editor unavailable");
        return false;
    }
    let ctl = ui.global::<WallEditController>();
    let index = ctl.get_line_index();
    let state = &mut *shared.borrow_mut();

    let Some((key, front, back, front_sector)) = state.app.map.as_ref().and_then(|m| {
        let key = m.lines.key_at_slot(index as u32)?;
        let line = m.lines.get(key)?;
        Some((key, line.front, line.back, line.front.sector?))
    }) else {
        return false;
    };
    let Some(front_heights) = side_heights(state, front_sector) else {
        return false;
    };
    let back_heights = back
        .and_then(|b| b.sector)
        .and_then(|k| side_heights(state, k));

    let Some(session) = state.app.begin_session() else {
        return false;
    };

    let selected_slot = default_slot_for(state, key, 0);
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
    ctl.set_textures(model(textures));
    ctl.set_selected_slot(selected_slot.into());

    state.wall_edit = Some(WallEditDraft {
        line: key,
        front,
        back,
        front_heights,
        back_heights,
        selected_slot,
        session,
    });
    render(ui, state);
    push_heights(ui, state);
    scroll_to_selected(ui, state);
    true
}

/// A sector's current heights, as the spin row's working copy.
fn side_heights(state: &SharedState, sector: SectorKey) -> Option<SideHeights> {
    let s = state.app.map.as_ref()?.sectors.get(sector)?;
    Some(SideHeights {
        sector,
        floor_h: s.floor_height,
        ceil_h: s.ceil_height,
    })
}

/// Show the active tab's sector heights in the spin row.
fn push_heights(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<WallEditController>();
    let Some(draft) = state.wall_edit.as_ref() else {
        return;
    };
    let heights = if ctl.get_side_tab() == 0 {
        Some(draft.front_heights)
    } else {
        draft.back_heights
    };
    if let Some(h) = heights {
        ctl.set_floor_h(h.floor_h);
        ctl.set_ceil_h(h.ceil_h);
    }
}

/// Write the draft to the map (no undo record) and refresh canvas, panels, preview.
fn write_through(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let damage = {
        let state = &mut *shared.borrow_mut();
        let Some(draft) = state.wall_edit.as_ref() else {
            return;
        };
        let Some(new_line) = state.app.map.as_ref().and_then(|m| draft_line(m, draft)) else {
            return;
        };
        let key = draft.line;
        let heights = [Some(draft.front_heights), draft.back_heights];
        let mut damage = state.app.set_line(key, new_line);
        for h in heights.into_iter().flatten() {
            let sector_dmg = state
                .app
                .map
                .as_ref()
                .and_then(|m| m.sectors.get(h.sector))
                .copied()
                .map(|old| {
                    state.app.set_sector(
                        h.sector,
                        Sector {
                            floor_height: h.floor_h,
                            ceil_height: h.ceil_h,
                            ..old
                        },
                    )
                })
                .unwrap_or(Damage::None);
            damage = damage.combine(sector_dmg);
        }
        damage
    };
    apply_damage(ui, shared, damage);
    panels::sync(ui, shared);
    render(ui, &shared.borrow());
}

/// The draft's sides over the map line's untouched fields.
fn draft_line(map: &EditorMap, draft: &WallEditDraft) -> Option<LineDef> {
    let l = map.lines.get(draft.line)?;
    Some(LineDef {
        v1: l.v1,
        v2: l.v2,
        flags: l.flags,
        special: l.special,
        tag: l.tag,
        front: draft.front,
        back: draft.back,
    })
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
    let Some(line) = map.lines.get(draft.line) else {
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

    let bands = draft_bands(map, draft, &side, is_front);
    let width_world = line_length(map, line).unwrap_or(0.0);
    let total: f32 = bands.iter().map(|(_, b)| b.height).sum();
    if width_world <= 0.0 || total <= 0.0 {
        ctl.set_wall_w(0.0);
        ctl.set_wall_h(0.0);
        ctl.set_bands(model(Vec::<BandRect>::new()));
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
    ctl.set_bands(model(rects));
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

/// Show the selected band's side offsets in the spin row.
fn push_offsets(ui: &EditorWindow, state: &SharedState) {
    let ctl = ui.global::<WallEditController>();
    let Some(draft) = state.wall_edit.as_ref() else {
        return;
    };
    let side = offset_side(draft);
    ctl.set_xoff(side.x_offset);
    ctl.set_yoff(side.y_offset);
}

/// Side owning the selected band: Front* slots -> front, else back.
fn offset_side(draft: &WallEditDraft) -> SideDef {
    if is_front_slot(draft.selected_slot) {
        draft.front
    } else {
        draft.back.unwrap_or(draft.front)
    }
}

fn offset_side_mut(draft: &mut WallEditDraft) -> Option<&mut SideDef> {
    if is_front_slot(draft.selected_slot) {
        Some(&mut draft.front)
    } else {
        draft.back.as_mut()
    }
}

fn is_front_slot(slot: TexSlot) -> bool {
    matches!(
        slot,
        TexSlot::FrontTop | TexSlot::FrontMid | TexSlot::FrontBottom
    )
}

/// Default slot for active side: topmost present band.
fn default_slot(ui: &EditorWindow, state: &SharedState) -> TexSlot {
    let side_tab = ui.global::<WallEditController>().get_side_tab();
    state
        .wall_edit
        .as_ref()
        .map(|d| default_slot_for(state, d.line, side_tab))
        .unwrap_or(TexSlot::FrontMid)
}

fn default_slot_for(state: &SharedState, line: LineKey, side_tab: i32) -> TexSlot {
    let is_front = side_tab == 0;
    let bands = state
        .app
        .map
        .as_ref()
        .and_then(|m| {
            let line = m.lines.get(line)?;
            let side = if is_front { line.front } else { line.back? };
            let own = m.sectors.get(side.sector?)?;
            let other_key = if is_front {
                line.back.and_then(|b| b.sector)
            } else {
                line.front.sector
            };
            let other = other_key.and_then(|k| m.sectors.get(k));
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
    ctl.set_active_index(entry_index(
        ctl.get_textures().iter().map(|e| e.name),
        tex.to_dwd_field(),
    ));
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
    let Some(side) = state.wall_edit.as_mut().and_then(offset_side_mut) else {
        return;
    };
    side.x_offset += dx;
    side.y_offset += dy;
}

/// Commit the session: map already holds the live edits; record them as one undo step.
fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    write_through(ui, shared);
    let state = &mut *shared.borrow_mut();
    if let Some(draft) = state.wall_edit.take() {
        state
            .app
            .commit_session(EditAction::EditLine, draft.session);
    }
}

/// Bands for the active side; the map already carries the written-through edits.
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
    let Some(own) = side.sector.and_then(|k| map.sectors.get(k)) else {
        return Vec::new();
    };
    let other = other_sector_idx.and_then(|k| map.sectors.get(k).copied());
    side_bands_tagged(own, other.as_ref(), side, is_front)
}
