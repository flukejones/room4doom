//! Wall editor popup: edits write through live; Apply = one undo step, close = revert.

use crate::app::session::{Damage, EditorSession};
use crate::boundary::TexSlot;
use crate::components::types::BandRect;
use crate::components::types::TexSlot as UiTexSlot;
use crate::dialogs::asset_picker::actions::{entry_index, texture_entries};
use crate::dialogs::drafts::DialogDrafts;
use crate::gfx::{WallBand, render_wall};
use crate::level_editor::preview::side_bands_tagged;
use crate::undo::{EditAction, EditSession};
use editor_core::geom::line_length;
use editor_core::{EditorMap, LineDef, LineKey, Name8, Sector, SectorKey, SideDef};

const KEY_UP: &str = "\u{f700}";
const KEY_DOWN: &str = "\u{f701}";
const KEY_LEFT: &str = "\u{f702}";
const KEY_RIGHT: &str = "\u{f703}";
const NUDGE_STEP: i32 = 1;
const NUDGE_STEP_SHIFT: i32 = 10;

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
    document: u64,
    line: LineKey,
    front: SideDef,
    back: Option<SideDef>,
    front_heights: SideHeights,
    back_heights: Option<SideHeights>,
    selected_slot: TexSlot,
    session: EditSession,
}

pub(crate) fn open(drafts: &mut DialogDrafts, state: &mut EditorSession, index: i32) -> bool {
    if !state.ensure_gfx() {
        return false;
    }
    let Some((key, front, back, front_sector)) = state.map_editor.map.as_ref().and_then(|m| {
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
    let Some(session) = state.map_editor.begin_session() else {
        return false;
    };
    let selected_slot = default_slot_for(state, key, 0);
    let EditorSession {
        gfx,
        assets,
        wad_data,
        ..
    } = state;
    let textures = texture_entries(
        gfx.as_mut().expect("ensured"),
        assets.as_ref().expect("ensured"),
        wad_data.as_ref().expect("ensured"),
        "",
    );
    drafts.wall.line_index = index;
    drafts.wall.has_back = back.is_some();
    drafts.wall.side_tab = 0;
    drafts.wall.textures = textures;
    drafts.wall.selected_slot = selected_slot.into();
    drafts.wall_edit = Some(WallEditDraft {
        document: state.document_generation,
        line: key,
        front,
        back,
        front_heights,
        back_heights,
        selected_slot,
        session,
    });
    render(drafts, state);
    push_heights(drafts);
    scroll_to_selected(drafts);
    true
}

pub(crate) fn close(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    drafts.wall_edit.take().map_or(Damage::None, |draft| {
        if draft.document == state.document_generation {
            state.map_editor.cancel_session(draft.session)
        } else {
            Damage::None
        }
    })
}

fn side_heights(state: &EditorSession, sector: SectorKey) -> Option<SideHeights> {
    let s = state.map_editor.map.as_ref()?.sectors.get(sector)?;
    Some(SideHeights {
        sector,
        floor_h: s.floor_height,
        ceil_h: s.ceil_height,
    })
}

fn push_heights(drafts: &mut DialogDrafts) {
    let Some(draft) = drafts.wall_edit.as_ref() else {
        return;
    };
    let heights = if drafts.wall.side_tab == 0 {
        Some(draft.front_heights)
    } else {
        draft.back_heights
    };
    if let Some(h) = heights {
        drafts.wall.floor_h = h.floor_h;
        drafts.wall.ceil_h = h.ceil_h;
    }
}

fn write_through(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    let Some(draft) = drafts.wall_edit.as_ref() else {
        return Damage::None;
    };
    if draft.document != state.document_generation {
        return Damage::None;
    }
    let Some(line) = state
        .map_editor
        .map
        .as_ref()
        .and_then(|m| draft_line(m, draft))
    else {
        return Damage::None;
    };
    let heights = [Some(draft.front_heights), draft.back_heights];
    let mut damage = state.map_editor.set_line(draft.line, line);
    for h in heights.into_iter().flatten() {
        let old = state
            .map_editor
            .map
            .as_ref()
            .and_then(|m| m.sectors.get(h.sector))
            .copied();
        if let Some(old) = old {
            damage = damage.combine(state.map_editor.set_sector(
                h.sector,
                Sector {
                    floor_height: h.floor_h,
                    ceil_height: h.ceil_h,
                    ..old
                },
            ));
        }
    }
    render(drafts, state);
    damage
}

fn draft_line(map: &EditorMap, draft: &WallEditDraft) -> Option<LineDef> {
    let l = map.lines.get(draft.line)?;
    Some(LineDef {
        front: draft.front,
        back: draft.back,
        ..*l
    })
}

fn render(drafts: &mut DialogDrafts, state: &EditorSession) {
    let Some(draft) = drafts.wall_edit.as_ref() else {
        return;
    };
    let is_front = drafts.wall.side_tab == 0;
    let Some(map) = state.map_editor.map.as_ref() else {
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
    let width_world = line_length(map, line).unwrap_or(0.);
    let total: f32 = bands.iter().map(|(_, b)| b.height).sum();
    if width_world <= 0. || total <= 0. {
        drafts.wall.wall_w = 0.;
        drafts.wall.wall_h = 0.;
        drafts.wall.bands = Vec::<BandRect>::new();
    } else {
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
        if let Some(image) = render_wall(
            state.assets.as_ref().expect("open ensured assets"),
            &plain,
            width_world,
            px * drafts.pixel_ratio,
        ) {
            drafts.wall.wall_img = image;
        }
        drafts.wall.wall_w = width_world * px;
        drafts.wall.wall_h = total * px;
        drafts.wall.bands = band_rects(&bands, px);
    }
    push_offsets(drafts);
}

fn push_offsets(drafts: &mut DialogDrafts) {
    if let Some(draft) = &drafts.wall_edit {
        let side = offset_side(draft);
        drafts.wall.xoff = side.x_offset;
        drafts.wall.yoff = side.y_offset;
    }
}

fn scroll_to_selected(drafts: &mut DialogDrafts) {
    if let Some(draft) = &drafts.wall_edit {
        drafts.wall.active_index = entry_index(
            drafts.wall.textures.iter().map(|e| &e.name),
            slot_tex(draft).to_dwd_field(),
        );
    }
}

pub(crate) fn side_changed(drafts: &mut DialogDrafts, state: &EditorSession) {
    if let Some(draft) = &mut drafts.wall_edit {
        draft.selected_slot = default_slot_for(state, draft.line, drafts.wall.side_tab);
        drafts.wall.selected_slot = draft.selected_slot.into();
    }
    render(drafts, state);
    push_heights(drafts);
    scroll_to_selected(drafts);
}

pub(crate) fn select_band(drafts: &mut DialogDrafts, slot: UiTexSlot) {
    if let Some(draft) = &mut drafts.wall_edit {
        draft.selected_slot = slot.into();
    }
    drafts.wall.selected_slot = slot;
    scroll_to_selected(drafts);
    push_offsets(drafts);
}

pub(crate) fn pick_texture(
    drafts: &mut DialogDrafts,
    state: &mut EditorSession,
    name: &str,
) -> Damage {
    if let Some(draft) = &mut drafts.wall_edit {
        set_slot_tex(
            draft,
            Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY),
        );
    }
    let damage = write_through(drafts, state);
    scroll_to_selected(drafts);
    damage
}

pub(crate) fn height_changed(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    if let Some(draft) = &mut drafts.wall_edit {
        let h = if drafts.wall.side_tab == 0 {
            Some(&mut draft.front_heights)
        } else {
            draft.back_heights.as_mut()
        };
        if let Some(h) = h {
            h.floor_h = drafts.wall.floor_h;
            h.ceil_h = drafts.wall.ceil_h;
        }
    }
    write_through(drafts, state)
}

pub(crate) fn offset_changed(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    if let Some(side) = drafts.wall_edit.as_mut().and_then(offset_side_mut) {
        side.x_offset = drafts.wall.xoff;
        side.y_offset = drafts.wall.yoff;
    }
    write_through(drafts, state)
}

pub(crate) fn key(
    drafts: &mut DialogDrafts,
    state: &mut EditorSession,
    key: &str,
    shift: bool,
) -> Damage {
    let step = if shift { NUDGE_STEP_SHIFT } else { NUDGE_STEP };
    let (dx, dy) = match key {
        KEY_LEFT => (-step, 0),
        KEY_RIGHT => (step, 0),
        KEY_UP => (0, -step),
        KEY_DOWN => (0, step),
        _ => return Damage::None,
    };
    if let Some(side) = drafts.wall_edit.as_mut().and_then(offset_side_mut) {
        side.x_offset += dx;
        side.y_offset += dy;
    }
    write_through(drafts, state)
}

pub(crate) fn apply(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    let damage = write_through(drafts, state);
    if let Some(draft) = drafts.wall_edit.take()
        && draft.document == state.document_generation
    {
        state
            .map_editor
            .commit_session(EditAction::EditLine, draft.session);
    }
    damage
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

fn default_slot_for(state: &EditorSession, line: LineKey, side_tab: i32) -> TexSlot {
    let is_front = side_tab == 0;
    let bands = state
        .map_editor
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_editor::LevelEditorState;
    use editor_core::ArenaKey as _;

    #[test]
    fn wall_popup_cancel_reverts_and_apply_records_one_undo_step() {
        let iwad = test_utils::doom1_wad_path();
        let wad = wad::WadData::new(&iwad);
        let original = editor_core::import_wad_map(&wad, "E1M1").unwrap();
        let (line, offset) = original
            .lines
            .iter()
            .find(|(_, line)| line.back.is_none() && line.front.sector.is_some())
            .map(|(key, line)| (key, line.front.x_offset))
            .unwrap();
        let slot = i32::try_from(line.slot()).unwrap();
        let mut app = LevelEditorState::new();
        app.load_map(original.clone(), "E1M1");
        let mut state = EditorSession::new(app, Some(iwad), Default::default());
        let mut drafts = DialogDrafts {
            pixel_ratio: 1.,
            ..Default::default()
        };

        assert!(open(&mut drafts, &mut state, slot));
        drafts.wall.xoff = offset + 4;
        offset_changed(&mut drafts, &mut state);
        assert_eq!(
            state.map_editor.map.as_ref().unwrap().lines[line]
                .front
                .x_offset,
            offset + 4
        );
        close(&mut drafts, &mut state);
        assert_eq!(state.map_editor.map.as_ref().unwrap(), &original);
        assert_eq!(state.map_editor.undo(), Damage::None);

        assert!(open(&mut drafts, &mut state, slot));
        for delta in [4, 8] {
            drafts.wall.xoff = offset + delta;
            offset_changed(&mut drafts, &mut state);
        }
        apply(&mut drafts, &mut state);
        close(&mut drafts, &mut state);
        assert_eq!(
            state.map_editor.map.as_ref().unwrap().lines[line]
                .front
                .x_offset,
            offset + 8
        );
        assert_eq!(state.map_editor.undo(), Damage::Edited);
        assert_eq!(state.map_editor.map.as_ref().unwrap(), &original);
        assert_eq!(state.map_editor.undo(), Damage::None);

        assert!(open(&mut drafts, &mut state, slot));
        drafts.wall.xoff = offset + 12;
        offset_changed(&mut drafts, &mut state);
        let replacement = editor_core::import_wad_map(&wad, "E1M2").unwrap();
        state.map_editor.load_map(replacement.clone(), "E1M2");
        state.reset_map();
        drafts.wall.xoff = offset + 16;
        offset_changed(&mut drafts, &mut state);
        close(&mut drafts, &mut state);
        assert_eq!(state.map_editor.map.as_ref().unwrap(), &replacement);
        assert_eq!(state.map_editor.undo(), Damage::None);
    }
}
