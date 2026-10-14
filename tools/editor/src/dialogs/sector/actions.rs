//! Live sector edits form one undo transaction; Cancel restores the snapshot.

use crate::app::session::{Damage, EditorSession};
use crate::assets::FLAT_SIDE;
use crate::boundary::FlatSlot;
use crate::components::types::FlatSlot as UiFlatSlot;
use crate::dialogs::{
    asset_picker::actions::{entry_index, flat_entries},
    drafts::DialogDrafts,
};
use crate::gfx::render_flat_square;
use crate::undo::{EditAction, EditSession};
use editor_core::{Name8, Sector, SectorKey};

pub(crate) struct SectorEditDraft {
    document: u64,
    sector_key: SectorKey,
    sector: Sector,
    selected_slot: FlatSlot,
    session: EditSession,
}

pub(crate) fn open(drafts: &mut DialogDrafts, state: &mut EditorSession, index: i32) -> bool {
    if !state.ensure_gfx() {
        return false;
    }
    let Some((key, sector)) = state.map_editor.map.as_ref().and_then(|m| {
        let key = m.sectors.key_at_slot(index as u32)?;
        Some((key, *m.sectors.get(key)?))
    }) else {
        return false;
    };
    let Some(session) = state.map_editor.begin_session() else {
        return false;
    };
    drafts.sector.sector_index = index;
    drafts.sector.floor_h = sector.floor_height;
    drafts.sector.ceil_h = sector.ceil_height;
    drafts.sector.selected_slot = FlatSlot::Ceil.into();
    drafts.sector.entries = flat_entries(
        state.gfx.as_mut().expect("ensured"),
        state.assets.as_ref().expect("ensured"),
        "",
    );
    drafts.sector_edit = Some(SectorEditDraft {
        document: state.document_generation,
        sector_key: key,
        sector,
        selected_slot: FlatSlot::Ceil,
        session,
    });
    render(drafts, state);
    scroll_to_selected(drafts);
    true
}

pub(crate) fn close(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    drafts.sector_edit.take().map_or(Damage::None, |draft| {
        if draft.document == state.document_generation {
            state.map_editor.cancel_session(draft.session)
        } else {
            Damage::None
        }
    })
}

fn write_through(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    let Some(draft) = &drafts.sector_edit else {
        return Damage::None;
    };
    if draft.document != state.document_generation {
        return Damage::None;
    }
    let damage = state.map_editor.set_sector(draft.sector_key, draft.sector);
    render(drafts, state);
    damage
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "Preview dimensions fit the bounded flat target."
)]
fn render(drafts: &mut DialogDrafts, state: &EditorSession) {
    let Some(draft) = &drafts.sector_edit else {
        return;
    };
    let physical = (FLAT_SIDE as f32 * drafts.pixel_ratio).ceil() as u32;
    let assets = state.assets.as_ref().expect("open ensured assets");
    drafts.sector.ceil_img = render_flat_square(assets, draft.sector.ceil_flat, physical);
    drafts.sector.floor_img = render_flat_square(assets, draft.sector.floor_flat, physical);
}

fn scroll_to_selected(drafts: &mut DialogDrafts) {
    if let Some(draft) = &drafts.sector_edit {
        let flat = match draft.selected_slot {
            FlatSlot::Floor => draft.sector.floor_flat,
            FlatSlot::Ceil => draft.sector.ceil_flat,
        };
        drafts.sector.active_index = entry_index(
            drafts.sector.entries.iter().map(|e| &e.name),
            flat.to_dwd_field(),
        );
    }
}

pub(crate) fn select_flat(drafts: &mut DialogDrafts, slot: UiFlatSlot) {
    if let Some(draft) = &mut drafts.sector_edit {
        draft.selected_slot = slot.into();
    }
    drafts.sector.selected_slot = slot;
    scroll_to_selected(drafts);
}

pub(crate) fn pick_flat(
    drafts: &mut DialogDrafts,
    state: &mut EditorSession,
    name: &str,
) -> Damage {
    if let Some(draft) = &mut drafts.sector_edit {
        let flat = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
        match draft.selected_slot {
            FlatSlot::Floor => draft.sector.floor_flat = flat,
            FlatSlot::Ceil => draft.sector.ceil_flat = flat,
        }
    }
    let damage = write_through(drafts, state);
    scroll_to_selected(drafts);
    damage
}

pub(crate) fn height_changed(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    if let Some(draft) = &mut drafts.sector_edit {
        draft.sector.floor_height = drafts.sector.floor_h;
        draft.sector.ceil_height = drafts.sector.ceil_h;
    }
    write_through(drafts, state)
}

pub(crate) fn apply(drafts: &mut DialogDrafts, state: &mut EditorSession) -> Damage {
    let damage = write_through(drafts, state);
    if let Some(draft) = drafts.sector_edit.take()
        && draft.document == state.document_generation
    {
        state
            .map_editor
            .commit_session(EditAction::EditSector, draft.session);
    }
    damage
}
