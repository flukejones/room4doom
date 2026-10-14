//! Populate asset choices without depending on a presentation context.

use crate::app::session::EditorSession;
use crate::assets::{EditorAssets, FLAT_SIDE};
use crate::boundary::{FlatSlot, TexSlot};
use crate::components::types::GfxEntry;
use crate::dialogs::drafts::DialogDrafts;
use crate::gfx::{GfxCache, image::UiImage};
use wad::WadData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TexBrowseTarget {
    LineSlot(TexSlot),
    SectorSlot(FlatSlot),
    Patch,
    Brush,
    DrawFloor,
    DrawCeil,
}

pub(crate) fn populate(
    drafts: &mut DialogDrafts,
    state: &mut EditorSession,
    target: TexBrowseTarget,
    current: Option<String>,
) -> bool {
    if !state.ensure_gfx() {
        return false;
    }
    drafts.browse_target = Some(target);
    drafts.browse_current = current;
    drafts.asset_picker.title = match target {
        TexBrowseTarget::LineSlot(TexSlot::FrontTop | TexSlot::BackTop) => "Wall - Upper",
        TexBrowseTarget::LineSlot(TexSlot::FrontMid | TexSlot::BackMid) => "Wall - Middle",
        TexBrowseTarget::LineSlot(TexSlot::FrontBottom | TexSlot::BackBottom) => "Wall - Lower",
        TexBrowseTarget::SectorSlot(FlatSlot::Floor) | TexBrowseTarget::DrawFloor => {
            "Sector - Floor"
        }
        TexBrowseTarget::SectorSlot(FlatSlot::Ceil) | TexBrowseTarget::DrawCeil => {
            "Sector - Ceiling"
        }
        TexBrowseTarget::Brush => "Textures",
        TexBrowseTarget::Patch => "Patches",
    }
    .into();
    drafts.asset_picker.filter.clear();
    rebuild_entries(drafts, state);
    true
}

pub(crate) fn rebuild_entries(drafts: &mut DialogDrafts, state: &mut EditorSession) {
    let filter = drafts.asset_picker.filter.to_uppercase();
    let Some(target) = drafts.browse_target else {
        return;
    };
    if !state.ensure_gfx() {
        return;
    }
    let EditorSession {
        gfx,
        assets,
        wad_data,
        ..
    } = state;
    let gfx = gfx.as_mut().expect("ensured above");
    let assets = assets.as_ref().expect("ensured above");
    let wad = wad_data.as_ref().expect("ensured above");
    let mut entries = match target {
        TexBrowseTarget::LineSlot(_) | TexBrowseTarget::Brush => {
            texture_entries(gfx, assets, wad, &filter)
        }
        TexBrowseTarget::SectorSlot(_) | TexBrowseTarget::DrawFloor | TexBrowseTarget::DrawCeil => {
            flat_entries(gfx, assets, &filter)
        }
        TexBrowseTarget::Patch => Vec::new(),
    };
    if target == TexBrowseTarget::Patch {
        let names = gfx.patch_names(wad).to_vec();
        for name in &names {
            if name_matches(name, &filter) {
                entries.push(patch_entry(gfx, assets, wad, name));
            }
        }
        for patch in assets.imported_patches() {
            let name = patch.name.as_str();
            if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) && name_matches(name, &filter) {
                entries.push(patch_entry(gfx, assets, wad, name));
            }
        }
    }
    drafts.asset_picker.active_index = drafts.browse_current.as_ref().map_or(-1, |name| {
        entry_index(entries.iter().map(|e| &e.name), name.trim())
    });
    drafts.asset_picker.entries = entries;
}

/// Browser entry for a named patch; zero size and blank thumb when lookup fails.
fn patch_entry(gfx: &mut GfxCache, assets: &EditorAssets, wad: &WadData, name: &str) -> GfxEntry {
    let (w, h) = gfx.patch_size(assets, wad, name).unwrap_or((0, 0));
    GfxEntry {
        name: name.into(),
        thumb: gfx.patch_image(assets, wad, name).unwrap_or_default(),
        tex_w: w as i32,
        tex_h: h as i32,
    }
}

/// Row index of `name` (ASCII case-insensitive) among `names`, or -1 when absent.
#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
pub(crate) fn entry_index<N: AsRef<str>>(names: impl IntoIterator<Item = N>, name: &str) -> i32 {
    names
        .into_iter()
        .position(|n| n.as_ref().eq_ignore_ascii_case(name))
        .map_or(-1, |i| i as i32)
}

/// Case-insensitive substring filter; `filter` must be pre-folded to uppercase once per rebuild, not per row.
pub(crate) fn name_matches(name: &str, filter: &str) -> bool {
    filter.is_empty() || name.to_uppercase().contains(filter)
}

/// Texture entries: "-" first, filtered by uppercased substring.
pub(crate) fn texture_entries(
    gfx: &mut GfxCache,
    assets: &EditorAssets,
    wad: &WadData,
    filter: &str,
) -> Vec<GfxEntry> {
    let mut entries = vec![GfxEntry {
        name: "-".into(),
        thumb: UiImage::default(),
        tex_w: 0,
        tex_h: 0,
    }];
    for (num, def) in assets.textures().iter().enumerate() {
        let name = def.name.as_str();
        if !name_matches(name, filter) {
            continue;
        }
        entries.push(GfxEntry {
            name: name.into(),
            thumb: gfx.texture_image(assets, wad, num),
            tex_w: def.width.max(0),
            tex_h: def.height.max(0),
        });
    }
    entries
}

/// Flat entries filtered by uppercased substring.
#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
pub(crate) fn flat_entries(
    gfx: &mut GfxCache,
    assets: &EditorAssets,
    filter: &str,
) -> Vec<GfxEntry> {
    let mut entries = Vec::with_capacity(assets.iwad_flats().len());
    for (num, flat) in assets.iwad_flats().iter().enumerate() {
        let name = flat.name.as_str();
        if !name_matches(name, filter) {
            continue;
        }
        entries.push(GfxEntry {
            name: name.into(),
            thumb: gfx.flat_image(assets, num),
            tex_w: FLAT_SIDE as i32,
            tex_h: FLAT_SIDE as i32,
        });
    }
    entries
}
