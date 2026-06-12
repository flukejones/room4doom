//! Texture/flat browser glue: "…" buttons open overlay; picks land in panel field.
//! Thumbnails from `GfxCache`, built lazily.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, VecModel};

use editor_core::Name8;
use wad::WadData;

use crate::SharedState;
use crate::assets::EditorAssets;
use crate::boundary::{FlatSlot, TexSlot};
use crate::generated::{
    DrawSettingsController, EditorWindow, GfxEntry, LinedefInspectorController,
    SectorInspectorController, StatusController, TexBrowserController, TexEditController,
};
use crate::gfx::GfxCache;
use crate::prefs::PopupWindow;
use crate::views::view_window::restore as restore_geom;

/// Which panel field a browser pick fills.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TexBrowseTarget {
    /// A line-side texture slot.
    LineSlot(TexSlot),
    /// A sector flat slot.
    SectorSlot(FlatSlot),
    /// Texture-editor patch picker; the pick adds a patch to the texture.
    Patch,
    /// The brush texture for newly drawn line middles.
    Brush,
    /// The draw-settings floor flat.
    DrawFloor,
    /// The draw-settings ceil flat.
    DrawCeil,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<LinedefInspectorController>()
        .on_populate_texture_browser(move |slot| {
            if let Some(ui) = weak.upgrade() {
                populate(&ui, &s, TexBrowseTarget::LineSlot(slot.into()));
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorInspectorController>()
        .on_populate_flat_browser(move |slot| {
            if let Some(ui) = weak.upgrade() {
                populate(&ui, &s, TexBrowseTarget::SectorSlot(slot.into()));
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<TexBrowserController>()
        .on_filter_edited(move || {
            if let Some(ui) = weak.upgrade() {
                rebuild_entries(&ui, &s);
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<TexBrowserController>().on_picked(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let target = s.borrow_mut().tex_browse_target.take();
        match target {
            Some(TexBrowseTarget::LineSlot(slot)) => {
                let ctl = ui.global::<LinedefInspectorController>();
                // Set field + touched flag so batch apply covers every selected line.
                let (mut front, mut back, mut t) =
                    (ctl.get_front(), ctl.get_back(), ctl.get_touched());
                match slot {
                    TexSlot::FrontTop => (front.top, t.front_top) = (name, true),
                    TexSlot::FrontMid => (front.mid, t.front_mid) = (name, true),
                    TexSlot::FrontBottom => (front.bottom, t.front_bottom) = (name, true),
                    TexSlot::BackTop => (back.top, t.back_top) = (name, true),
                    TexSlot::BackMid => (back.mid, t.back_mid) = (name, true),
                    TexSlot::BackBottom => (back.bottom, t.back_bottom) = (name, true),
                }
                ctl.set_front(front);
                ctl.set_back(back);
                ctl.set_touched(t);
                ctl.invoke_apply();
            }
            Some(TexBrowseTarget::SectorSlot(slot)) => {
                let ctl = ui.global::<SectorInspectorController>();
                match slot {
                    FlatSlot::Floor => ctl.set_floor_flat(name),
                    FlatSlot::Ceil => ctl.set_ceil_flat(name),
                }
                ctl.invoke_apply();
            }
            Some(TexBrowseTarget::Patch) => {
                let ctl = ui.global::<TexEditController>();
                ctl.set_new_patch_name(name);
                ctl.invoke_patch_add();
            }
            Some(TexBrowseTarget::Brush) => {
                set_brush(&ui, &s, &name);
            }
            Some(TexBrowseTarget::DrawFloor) => {
                let ctl = ui.global::<DrawSettingsController>();
                ctl.set_floor_flat(name);
                ctl.invoke_apply();
            }
            Some(TexBrowseTarget::DrawCeil) => {
                let ctl = ui.global::<DrawSettingsController>();
                ctl.set_ceil_flat(name);
                ctl.invoke_apply();
            }
            None => {}
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>().on_brush_browse(move || {
        if let Some(ui) = weak.upgrade()
            && populate(&ui, &s, TexBrowseTarget::Brush)
        {
            ui.global::<TexBrowserController>()
                .set_browser_visible(true);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<TexEditController>()
        .on_populate_patch_browser(move || {
            if let Some(ui) = weak.upgrade() {
                populate(&ui, &s, TexBrowseTarget::Patch);
            }
        });

    push_brush_chip(ui, shared);
}

/// Set draw brush texture; empty/invalid name clears it.
pub(crate) fn set_brush(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, name: &str) {
    let tex = Name8::from_dwd_field(name.trim()).unwrap_or(Name8::EMPTY);
    shared.borrow_mut().app.draw_brush.wall_tex = tex;
    push_brush_chip(ui, shared);
}

pub(crate) fn push_brush_chip(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let tex = shared.borrow().app.draw_brush.wall_tex;
    let label = if tex == Name8::EMPTY {
        "brush: -".to_owned()
    } else {
        format!("brush: {}", tex.as_str())
    };
    ui.global::<StatusController>().set_brush(label.into());
}

/// Fill browser for `target`. Returns false when no IWAD is open.
pub(crate) fn populate(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    target: TexBrowseTarget,
) -> bool {
    if !shared.borrow_mut().ensure_gfx() {
        log::warn!("no IWAD open - texture browser unavailable");
        return false;
    }
    restore_geom(ui, shared, PopupWindow::Browser);
    shared.borrow_mut().tex_browse_target = Some(target);
    let browser = ui.global::<TexBrowserController>();
    browser.set_title(match target {
        TexBrowseTarget::LineSlot(_) | TexBrowseTarget::Brush => "Textures".into(),
        TexBrowseTarget::SectorSlot(_) | TexBrowseTarget::DrawFloor | TexBrowseTarget::DrawCeil => {
            "Flats".into()
        }
        TexBrowseTarget::Patch => "Patches".into(),
    });
    browser.set_filter("".into());
    rebuild_entries(ui, shared);
    true
}

fn rebuild_entries(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let browser = ui.global::<TexBrowserController>();
    let filter = browser.get_filter().to_uppercase();
    let state = &mut *shared.borrow_mut();
    let Some(target) = state.tex_browse_target else {
        return;
    };
    if !state.ensure_gfx() {
        return;
    }
    let SharedState {
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
        let names: Vec<String> = gfx.patch_names(wad).to_vec();
        for name in &names {
            if !matches(name, &filter) {
                continue;
            }
            let (w, h) = gfx.patch_size(assets, wad, name).unwrap_or((0, 0));
            entries.push(GfxEntry {
                name: name.clone().into(),
                thumb: gfx.patch_image(assets, wad, name).unwrap_or_default(),
                tex_w: w as i32,
                tex_h: h as i32,
            });
        }
        // Imported patches absent from PNAMES.
        for patch in assets.imported_patches() {
            let name = patch.name.as_str();
            if names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                continue;
            }
            if !matches(name, &filter) {
                continue;
            }
            let (w, h) = gfx.patch_size(assets, wad, name).unwrap_or((0, 0));
            entries.push(GfxEntry {
                name: name.into(),
                thumb: gfx.patch_image(assets, wad, name).unwrap_or_default(),
                tex_w: w as i32,
                tex_h: h as i32,
            });
        }
    }
    let active = current_slot_value(ui, target)
        .and_then(|v| {
            entries
                .iter()
                .position(|e| e.name.as_str().eq_ignore_ascii_case(v.trim()))
        })
        .map_or(-1, |i| i as i32);
    browser.set_entries(slint::ModelRc::new(VecModel::from(entries)));
    browser.set_active_index(active);
}

/// Current texture/flat name for the slot; `None` for patch picker / Brush.
fn current_slot_value(ui: &EditorWindow, target: TexBrowseTarget) -> Option<String> {
    match target {
        TexBrowseTarget::LineSlot(slot) => {
            let ctl = ui.global::<LinedefInspectorController>();
            let (front, back) = (ctl.get_front(), ctl.get_back());
            Some(match slot {
                TexSlot::FrontTop => front.top,
                TexSlot::FrontMid => front.mid,
                TexSlot::FrontBottom => front.bottom,
                TexSlot::BackTop => back.top,
                TexSlot::BackMid => back.mid,
                TexSlot::BackBottom => back.bottom,
            })
        }
        TexBrowseTarget::SectorSlot(slot) => {
            let ctl = ui.global::<SectorInspectorController>();
            Some(match slot {
                FlatSlot::Floor => ctl.get_floor_flat(),
                FlatSlot::Ceil => ctl.get_ceil_flat(),
            })
        }
        TexBrowseTarget::DrawFloor => Some(ui.global::<DrawSettingsController>().get_floor_flat()),
        TexBrowseTarget::DrawCeil => Some(ui.global::<DrawSettingsController>().get_ceil_flat()),
        TexBrowseTarget::Patch | TexBrowseTarget::Brush => None,
    }
    .map(|s| s.to_string())
}

fn matches(name: &str, filter: &str) -> bool {
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
        thumb: slint::Image::default(),
        tex_w: 0,
        tex_h: 0,
    }];
    for (num, def) in assets.textures().iter().enumerate() {
        let name = def.name.as_str();
        if !matches(name, filter) {
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
pub(crate) fn flat_entries(
    gfx: &mut GfxCache,
    assets: &EditorAssets,
    filter: &str,
) -> Vec<GfxEntry> {
    let mut entries = Vec::with_capacity(assets.iwad_flats().len());
    for (num, flat) in assets.iwad_flats().iter().enumerate() {
        let name = flat.name.as_str();
        if !matches(name, filter) {
            continue;
        }
        entries.push(GfxEntry {
            name: name.into(),
            thumb: gfx.flat_image(assets, num),
            tex_w: 64,
            tex_h: 64,
        });
    }
    entries
}
