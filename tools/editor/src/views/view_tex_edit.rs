//! Texture editor glue: composite textures, patches, animations. No project: seeds an in-memory draft from the IWAD, Save creates a project on disk. Project open: edits texture set 1 directly.

use std::cell::RefCell;
use std::fs::File;
use std::mem;
use std::path::Path;
use std::rc::Rc;

use editor_core::{AnimDef, ImportedPatch, Name8, PatchPlacement, TextureDef};
use slint::ComponentHandle as _;

use super::{KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, NUDGE_STEP, NUDGE_STEP_SHIFT, model};
use crate::SharedState;
use crate::assets::palette::nearest_palette_index;
use crate::assets::{EditorAssets, encode_patch, patch_dims};
use crate::generated::{AnimRow, EditorWindow, Tabs, TexEditController, TexPatchRow};
use crate::gfx::compose_texture_highlight;

/// Default size for new textures.
const NEW_TEXTURE_SIZE: (i32, i32) = (128, 128);
/// Preview zoom cap (texel→px); keeps tiny textures legible.
const TEXEDIT_MAX_ZOOM: f32 = 8.0;
/// Offset applied to pasted patches so they don't sit atop the original.
const PATCH_PASTE_OFFSET: i32 = 8;

/// An in-progress patch drag: which placement and the grab offset in texels.
pub struct TexDrag {
    patch: usize,
    grab: [i32; 2],
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_tab_selected(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        if index != ui.global::<Tabs>().get_textures() {
            return;
        }
        if !ensure_draft(&mut s.borrow_mut()) {
            log::warn!("no WAD loaded - texture editor has nothing to show");
        }
        sync_list(&ui, &s);
        sync_anims(&ui, &s);
        select(&ui, &s, 0);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let ctl = ui.global::<TexEditController>();
    ctl.on_select({
        let (weak, s) = (weak.clone(), s.clone());
        move |index| {
            if let Some(ui) = weak.upgrade() {
                select(&ui, &s, index);
            }
        }
    });

    ctl.on_texedit_key({
        let (weak, s) = (weak.clone(), s.clone());
        move |key, shift, ctrl| {
            let Some(ui) = weak.upgrade() else { return };
            texedit_key(&ui, &s, &key, shift, ctrl);
        }
    });

    ctl.on_new_texture({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let index = {
                let state = &mut *s.borrow_mut();
                record(state);
                let Some(set) = textures_set_mut(state) else {
                    return;
                };
                set.push(TextureDef {
                    name: Name8::new("NEWTEX").expect("known-valid name"),
                    width: NEW_TEXTURE_SIZE.0,
                    height: NEW_TEXTURE_SIZE.1,
                    patches: Vec::new(),
                });
                (set.len() - 1) as i32
            };
            if let Some(ui) = weak.upgrade() {
                sync_list(&ui, &s);
                select(&ui, &s, index);
            }
        }
    });

    ctl.on_dup_texture({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            let index = {
                let state = &mut *s.borrow_mut();
                record(state);
                let Some(set) = textures_set_mut(state) else {
                    return;
                };
                let Some(src) = set.get(current as usize) else {
                    return;
                };
                let copy = TextureDef {
                    name: src.name,
                    width: src.width,
                    height: src.height,
                    patches: src.patches.clone(),
                };
                set.push(copy);
                (set.len() - 1) as i32
            };
            sync_list(&ui, &s);
            select(&ui, &s, index);
        }
    });

    ctl.on_del_texture({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            {
                let state = &mut *s.borrow_mut();
                record(state);
                let Some(set) = textures_set_mut(state) else {
                    return;
                };
                if (current as usize) < set.len() {
                    set.remove(current as usize);
                }
            }
            sync_list(&ui, &s);
            select(&ui, &s, 0);
        }
    });

    ctl.on_size_edited({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<TexEditController>();
            let current = ctl.get_current();
            let (w, h) = (
                ctl.get_tex_width().trim().parse::<i32>(),
                ctl.get_tex_height().trim().parse::<i32>(),
            );
            if !edit_texture(&s, current, |def| {
                if let Ok(w) = w {
                    def.width = w.clamp(1, 4096);
                }
                if let Ok(h) = h {
                    def.height = h.clamp(1, 4096);
                }
            }) {
                return;
            }
            refresh_preview(&ui, &s, current);
        }
    });

    ctl.on_patch_edited({
        let (weak, s) = (weak.clone(), s.clone());
        move |index, x, y| {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            if !edit_texture(&s, current, |def| {
                let Some(patch) = def.patches.get_mut(index as usize) else {
                    return;
                };
                if let Ok(x) = x.trim().parse() {
                    patch.origin_x = x;
                }
                if let Ok(y) = y.trim().parse() {
                    patch.origin_y = y;
                }
            }) {
                return;
            }
            refresh_preview(&ui, &s, current);
        }
    });

    ctl.on_patch_del({
        let (weak, s) = (weak.clone(), s.clone());
        move |index| {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            if !edit_texture(&s, current, |def| {
                if (index as usize) < def.patches.len() {
                    def.patches.remove(index as usize);
                }
            }) {
                return;
            }
            select(&ui, &s, current);
        }
    });

    ctl.on_patch_add({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<TexEditController>();
            let current = ctl.get_current();
            let Ok(name) = Name8::new(ctl.get_new_patch_name().trim()) else {
                ctl.set_status("invalid patch name".into());
                return;
            };
            if !edit_texture(&s, current, |def| {
                def.patches.push(PatchPlacement {
                    origin_x: 0,
                    origin_y: 0,
                    patch: name,
                    step_dir: 1,
                    colormap: 0,
                });
            }) {
                return;
            }
            select(&ui, &s, current); // keep typed name for repeated adds
        }
    });

    ctl.on_import_patch({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            // No borrow held during dialog; it pumps its own event loop.
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .pick_file()
            else {
                return;
            };
            let Some(ui) = weak.upgrade() else { return };
            import_png_patch(&ui, &s, &path);
        }
    });

    ctl.on_undo({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                undo_textures(&ui, &s);
            }
        }
    });

    ctl.on_save_project({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            // No borrow held during pick dialog.
            if s.borrow().project.is_none() {
                let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose a folder for the new project")
                    .pick_folder()
                else {
                    return;
                };
                if let Err(msg) = materialise_draft(&s, &dir) {
                    ui.global::<TexEditController>().set_status(msg.into());
                    return;
                }
            }
            let state = s.borrow();
            let Some(project) = &state.project else {
                return;
            };
            match project.save() {
                Ok(()) => ui
                    .global::<TexEditController>()
                    .set_status("saved project".into()),
                Err(e) => {
                    log::error!("project save: {e}");
                    ui.global::<TexEditController>()
                        .set_status("save failed".into());
                }
            }
        }
    });

    ctl.on_preview_resized({
        let (weak, s) = (weak.clone(), s.clone());
        move |w, h| {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            recompute_zoom(&ui, &s, current, w, h);
        }
    });

    ctl.on_preview_press({
        let (weak, s) = (weak.clone(), s.clone());
        move |x, y| {
            if let Some(ui) = weak.upgrade() {
                preview_press(&ui, &s, x, y);
            }
        }
    });

    ctl.on_preview_move({
        let (weak, s) = (weak.clone(), s.clone());
        move |x, y| {
            if let Some(ui) = weak.upgrade() {
                preview_move(&ui, &s, x, y);
            }
        }
    });

    ctl.on_preview_release({
        let s = s.clone();
        move || {
            s.borrow_mut().texedit.drag = None;
        }
    });

    ctl.on_colormap_changed({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let current = ui.global::<TexEditController>().get_current();
            refresh_preview(&ui, &s, current);
        }
    });

    ctl.on_anim_add({
        let (weak, s) = (weak.clone(), s.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let ctl = ui.global::<TexEditController>();
            let (Ok(start), Ok(end)) = (
                Name8::new(ctl.get_new_anim_start().trim()),
                Name8::new(ctl.get_new_anim_end().trim()),
            ) else {
                ctl.set_status("animation needs valid start and end names".into());
                return;
            };
            let speed = ctl.get_new_anim_speed().trim().parse::<i32>().unwrap_or(8);
            if !edit_anims(&s, |anims| {
                anims.push(AnimDef {
                    is_texture: ctl.get_new_anim_is_texture(),
                    start,
                    end,
                    speed,
                });
            }) {
                return;
            }
            ctl.set_new_anim_start("".into());
            ctl.set_new_anim_end("".into());
            sync_anims(&ui, &s);
        }
    });

    ctl.on_anim_del({
        let (weak, s) = (weak.clone(), s.clone());
        move |index| {
            let Some(ui) = weak.upgrade() else { return };
            if !edit_anims(&s, |anims| {
                let i = index as usize;
                if i < anims.len() {
                    anims.remove(i);
                }
            }) {
                return;
            }
            sync_anims(&ui, &s);
        }
    });

    ctl.on_anim_kind_toggled({
        let (weak, s) = (weak.clone(), s.clone());
        move |index| {
            let Some(ui) = weak.upgrade() else { return };
            if !edit_anims(&s, |anims| {
                if let Some(anim) = anims.get_mut(index as usize) {
                    anim.is_texture = !anim.is_texture;
                }
            }) {
                return;
            }
            sync_anims(&ui, &s);
        }
    });

    ctl.on_anim_edited({
        let s = s.clone();
        move |index, start, end, speed| {
            edit_anims(&s, |anims| {
                let Some(anim) = anims.get_mut(index as usize) else {
                    return;
                };
                if let Ok(start) = Name8::new(start.trim()) {
                    anim.start = start;
                }
                if let Ok(end) = Name8::new(end.trim()) {
                    anim.end = end;
                }
                if let Ok(speed) = speed.trim().parse() {
                    anim.speed = speed;
                }
            });
        }
    });
}

/// Fit texture into preview pane at integer zoom (capped), push dims.
fn recompute_zoom(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    index: i32,
    w: f32,
    h: f32,
) {
    let dims = {
        let state = shared.borrow();
        textures_set_ref(&state)
            .and_then(|set| set.get(index as usize))
            .map(|d| (d.width.max(1) as f32, d.height.max(1) as f32))
    };
    let Some((tw, th)) = dims else { return };
    // Small margin inside the pane.
    let avail_w = (w - 16.0).max(1.0); // 8px margin each side
    let avail_h = (h - 16.0).max(1.0);
    let zoom = (avail_w / tw)
        .min(avail_h / th)
        .clamp(1.0, TEXEDIT_MAX_ZOOM)
        .floor();
    shared.borrow_mut().texedit.zoom = zoom;
    let ctl = ui.global::<TexEditController>();
    ctl.set_preview_w(tw * zoom);
    ctl.set_preview_h(th * zoom);
}

fn preview_texel(state: &SharedState, x: f32, y: f32) -> [i32; 2] {
    let zoom = state.texedit.zoom.max(1.0);
    [(x / zoom).floor() as i32, (y / zoom).floor() as i32]
}

/// Press: hit-test topmost patch, select it, begin drag (records undo step).
fn preview_press(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, x: f32, y: f32) {
    let ctl = ui.global::<TexEditController>();
    let current = ctl.get_current();
    let mut hit_name: Option<slint::SharedString> = None;
    let refresh = {
        let state = &mut *shared.borrow_mut();
        let texel = preview_texel(state, x, y);
        match drag_start(state, current, texel) {
            Some(patch) => {
                ctl.set_selected_patch(patch as i32);
                hit_name = patch_name_at(state, current, patch).map(Into::into);
                true
            }
            None => false,
        }
    };
    if let Some(name) = hit_name {
        ctl.set_new_patch_name(name);
    }
    if refresh {
        select(ui, shared, current);
    }
}

/// Move: reposition dragged patch if drag is active.
fn preview_move(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, x: f32, y: f32) {
    let current = ui.global::<TexEditController>().get_current();
    let refresh = {
        let state = &mut *shared.borrow_mut();
        if state.texedit.drag.is_none() {
            return;
        }
        let texel = preview_texel(state, x, y);
        drag_move(state, current, texel)
    };
    if refresh {
        select(ui, shared, current);
    }
}

fn patch_name_at(state: &SharedState, current: i32, patch: usize) -> Option<String> {
    let set = textures_set_ref(state)?;
    let def = set.get(usize::try_from(current).ok()?)?;
    Some(def.patches.get(patch)?.patch.as_str().to_owned())
}

/// Ctrl-Z/Y: undo/redo; C/X/V: patch clipboard; arrows: nudge patch or navigate list.
fn texedit_key(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    key: &str,
    shift: bool,
    ctrl: bool,
) {
    let ctl = ui.global::<TexEditController>();
    let current = ctl.get_current();
    let selected = ctl.get_selected_patch();

    if ctrl {
        match key {
            "z" if shift => redo_textures(ui, shared),
            "z" => undo_textures(ui, shared),
            "y" => redo_textures(ui, shared),
            "c" => copy_patch(ui, shared),
            "x" => cut_patch(ui, shared),
            "v" => paste_patches(ui, shared),
            _ => {}
        }
        return;
    }

    if selected >= 0 {
        let step = if shift { NUDGE_STEP_SHIFT } else { NUDGE_STEP };
        let (dx, dy) = match key {
            KEY_LEFT => (-step, 0),
            KEY_RIGHT => (step, 0),
            KEY_UP => (0, -step),
            KEY_DOWN => (0, step),
            _ => return,
        };
        if !edit_texture(shared, current, |def| {
            if let Some(p) = def.patches.get_mut(selected as usize) {
                p.origin_x += dx;
                p.origin_y += dy;
            }
        }) {
            return;
        }
        select(ui, shared, current);
        return;
    }

    // No patch selected: Up/Down navigate the list.
    let delta = match key {
        KEY_UP => -1,
        KEY_DOWN => 1,
        _ => return,
    };
    let count = textures_set_ref(&shared.borrow())
        .map(<[_]>::len)
        .unwrap_or(0) as i32;
    if count == 0 {
        return;
    }
    let next = (current + delta).clamp(0, count - 1);
    if next != current {
        select(ui, shared, next);
    }
}

fn copy_patch(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<TexEditController>();
    let (current, selected) = (ctl.get_current(), ctl.get_selected_patch());
    if selected < 0 {
        return;
    }
    let state = &mut *shared.borrow_mut();
    let Some(def) = textures_set_ref(state).and_then(|s| s.get(current as usize)) else {
        return;
    };
    if let Some(p) = def.patches.get(selected as usize) {
        state.texedit.clipboard = vec![*p];
    }
}

fn cut_patch(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    copy_patch(ui, shared);
    let ctl = ui.global::<TexEditController>();
    let (current, selected) = (ctl.get_current(), ctl.get_selected_patch());
    if selected < 0 {
        return;
    }
    if !edit_texture(shared, current, |def| {
        if (selected as usize) < def.patches.len() {
            def.patches.remove(selected as usize);
        }
    }) {
        return;
    }
    ctl.set_selected_patch(-1);
    select(ui, shared, current);
}

/// Append clipboard patches offset by `PATCH_PASTE_OFFSET`; select last.
fn paste_patches(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<TexEditController>();
    let current = ctl.get_current();
    let clips = mem::take(&mut shared.borrow_mut().texedit.clipboard);
    if clips.is_empty() {
        return;
    }
    let mut new_selected = -1;
    let edited = edit_texture(shared, current, |def| {
        for mut p in clips.iter().copied() {
            p.origin_x += PATCH_PASTE_OFFSET;
            p.origin_y += PATCH_PASTE_OFFSET;
            def.patches.push(p);
        }
        new_selected = def.patches.len() as i32 - 1;
    });
    shared.borrow_mut().texedit.clipboard = clips;
    if !edited {
        return;
    }
    ctl.set_selected_patch(new_selected);
    select(ui, shared, current);
}

/// Hit-test topmost patch at `texel`, record drag. Returns hit index.
fn drag_start(state: &mut SharedState, current: i32, texel: [i32; 2]) -> Option<usize> {
    let (wad, imported, def) = wad_and_def(state, current)?;
    let mut hit = None;
    for (i, p) in def.patches.iter().enumerate().rev() {
        let Some((pw, ph)) = patch_dims(imported, wad, p.patch.as_str()) else {
            continue;
        };
        let inside = texel[0] >= p.origin_x
            && texel[0] < p.origin_x + pw as i32
            && texel[1] >= p.origin_y
            && texel[1] < p.origin_y + ph as i32;
        if inside {
            hit = Some((i, [texel[0] - p.origin_x, texel[1] - p.origin_y]));
            break;
        }
    }
    let (patch, grab) = hit?;
    record(state);
    state.texedit.drag = Some(TexDrag {
        patch,
        grab,
    });
    Some(patch)
}

fn drag_move(state: &mut SharedState, current: i32, texel: [i32; 2]) -> bool {
    let Some(drag) = &state.texedit.drag else {
        return false;
    };
    let (patch, grab) = (drag.patch, drag.grab);
    let Some(def) = texture_mut(state, current) else {
        return false;
    };
    let Some(placement) = def.patches.get_mut(patch) else {
        return false;
    };
    placement.origin_x = texel[0] - grab[0];
    placement.origin_y = texel[1] - grab[1];
    true
}

/// Seed draft from IWAD when no project is open; false if no WAD loaded.
fn ensure_draft(state: &mut SharedState) -> bool {
    state.ensure_assets()
}

/// Create project at `dir` from IWAD + assets; returns err string on failure.
fn materialise_draft(shared: &Rc<RefCell<SharedState>>, dir: &Path) -> Result<(), &'static str> {
    let state = &mut *shared.borrow_mut();
    if !state.ensure_assets() {
        return Err("no WAD loaded");
    }
    let Some(iwad) = state.iwad.clone() else {
        return Err("no IWAD path to base the project on");
    };
    let wad = state.wad_data.as_ref().expect("ensured above");
    let mut project = editor_core::Project::create(dir, &iwad, wad).map_err(|e| {
        log::error!("create project: {e}");
        "could not create project"
    })?;

    state
        .assets
        .as_mut()
        .expect("ensured above")
        .write_into(&mut project);
    state.project = Some(project);
    Ok(())
}

fn textures_set_ref(state: &SharedState) -> Option<&[TextureDef]> {
    state.assets.as_ref().map(EditorAssets::textures)
}

fn textures_set_mut(state: &mut SharedState) -> Option<&mut Vec<TextureDef>> {
    state.assets.as_mut().map(EditorAssets::textures_vec_mut)
}

fn animations_ref(state: &SharedState) -> Option<&[AnimDef]> {
    state.assets.as_ref().map(EditorAssets::animations)
}

fn animations_mut(state: &mut SharedState) -> Option<&mut Vec<AnimDef>> {
    state.assets.as_mut().map(EditorAssets::animations_vec_mut)
}

/// Record an undo step, then apply `edit` to texture `index`; false when the texture is missing.
fn edit_texture(
    shared: &Rc<RefCell<SharedState>>,
    index: i32,
    edit: impl FnOnce(&mut TextureDef),
) -> bool {
    let state = &mut *shared.borrow_mut();
    record(state);
    let Some(def) = texture_mut(state, index) else {
        return false;
    };
    edit(def);
    true
}

/// Record one undo step, then apply `edit` to the animation list; false when no assets are loaded.
fn edit_anims(shared: &Rc<RefCell<SharedState>>, edit: impl FnOnce(&mut Vec<AnimDef>)) -> bool {
    let mut state = shared.borrow_mut();
    record(&mut state);
    let Some(anims) = animations_mut(&mut state) else {
        return false;
    };
    edit(anims);
    true
}

fn wad_and_def(
    state: &mut SharedState,
    index: i32,
) -> Option<(&wad::WadData, &[ImportedPatch], &TextureDef)> {
    if !state.ensure_wad() {
        return None;
    }
    let SharedState {
        wad_data,
        assets,
        ..
    } = state;
    let wad = wad_data.as_ref()?;
    let assets = assets.as_ref()?;
    let def = assets.textures().get(usize::try_from(index).ok()?)?;
    Some((wad, assets.imported_patches(), def))
}

fn record(state: &mut SharedState) {
    let SharedState {
        texedit,
        assets,
        ..
    } = state;
    if let Some(assets) = assets.as_ref() {
        texedit.history.record(assets);
    }
}

fn undo_textures(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    restore_snapshot(ui, shared, false);
}

fn redo_textures(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    restore_snapshot(ui, shared, true);
}

fn restore_snapshot(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, redo: bool) {
    let len = {
        let state = &mut *shared.borrow_mut();
        let SharedState {
            texedit,
            assets,
            ..
        } = state;
        let Some(assets) = assets.as_mut() else {
            return;
        };
        let stepped = if redo {
            texedit.history.redo(assets)
        } else {
            texedit.history.undo(assets)
        };
        match stepped {
            Some(len) => len,
            None => return,
        }
    };
    let current = ui
        .global::<TexEditController>()
        .get_current()
        .min(len as i32 - 1)
        .max(0);
    sync_list(ui, shared);
    sync_anims(ui, shared);
    select(ui, shared, current);
}

fn texture_mut(state: &mut SharedState, index: i32) -> Option<&mut TextureDef> {
    let i = usize::try_from(index).ok()?;
    state.assets.as_mut()?.texture_mut(i)
}

fn sync_list(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    // Edits go through `texture_mut`; rebuild index after the batch settles.
    if let Some(assets) = shared.borrow_mut().assets.as_mut() {
        assets.refresh_texture_index();
    }
    let names: Vec<slint::SharedString> = {
        let state = shared.borrow();
        textures_set_ref(&state)
            .map(|set| {
                set.iter()
                    .map(|t| slint::SharedString::from(t.name.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    };
    ui.global::<TexEditController>().set_textures(model(names));
    sync_set_title(ui, shared);
}

fn sync_set_title(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let state = shared.borrow();
    let title = match &state.assets {
        Some(assets) => assets
            .group_label(assets.active_group())
            .unwrap_or_default(),
        None => String::new(),
    };
    ui.global::<TexEditController>().set_set_title(title.into());
}

/// Open texture set `index`; switch to its tab.
pub(crate) fn open_set(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, index: usize) {
    {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_assets() {
            return;
        }
        let Some(assets) = state.assets.as_mut() else {
            return;
        };
        assets.set_active_group(index);
    }
    ui.set_active_tab(ui.global::<Tabs>().get_textures());
    sync_list(ui, shared);
    sync_anims(ui, shared);
    select(ui, shared, 0);
}

fn sync_anims(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let rows: Vec<AnimRow> = {
        let state = shared.borrow();
        animations_ref(&state)
            .map(|anims| {
                anims
                    .iter()
                    .map(|a| AnimRow {
                        kind: if a.is_texture { "tex" } else { "flat" }.into(),
                        start: a.start.as_str().into(),
                        end: a.end.as_str().into(),
                        speed: slint::format!("{}", a.speed),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    ui.global::<TexEditController>().set_anims(model(rows));
}

/// Import PNG: decode → quantize to IWAD palette → encode as picture lump → register.
fn import_png_patch(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, path: &Path) {
    let ctl = ui.global::<TexEditController>();
    let status = |msg: &str| ctl.set_status(msg.into());

    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        status("PNG name is not valid text");
        return;
    };
    let Ok(name) = Name8::new(stem) else {
        status("PNG name is not a valid 8-char patch name");
        return;
    };
    let (width, height, rgba) = match decode_png_rgba8(path) {
        Ok(decoded) => decoded,
        Err(e) => {
            log::warn!("PNG import {}: {e}", path.display());
            status("could not decode PNG");
            return;
        }
    };

    {
        let state = &mut *shared.borrow_mut();
        if !ensure_draft(state) {
            status("no WAD palette to quantize against");
            return;
        }
        if state
            .wad_data
            .as_ref()
            .and_then(|w| w.get_lump(name.as_str()))
            .is_some()
        {
            log::warn!("imported patch {} shadows an IWAD lump", name.as_str());
        }
        let Some(assets) = state.assets.as_mut() else {
            status("no WAD loaded to import against");
            return;
        };
        let indices = quantize_to_indices(&rgba, assets.palette());
        let lump = encode_patch(width, height, &indices);
        if let Err(msg) = assets.import_patch(ImportedPatch {
            name,
            lump,
        }) {
            status(msg);
            return;
        }
    }

    status(&format!("imported {} ({width}x{height})", name.as_str()));
    let current = ctl.get_current();
    refresh_preview(ui, shared, current);
}

/// Decode PNG to straight RGBA8, expanding indexed/low-bit images.
fn decode_png_rgba8(path: &Path) -> Result<(usize, usize, Vec<u8>), png::DecodingError> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (width, height) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    buf.truncate(info.buffer_size());

    // After normalize_to_color8, indexed → RGB(A); Indexed arm is defensive.
    let mut rgba = vec![0u8; width * height * 4];
    for (pixel, src) in buf.chunks_exact(channels).enumerate() {
        let at = pixel * 4;
        match info.color_type {
            png::ColorType::Grayscale | png::ColorType::Indexed => {
                rgba[at] = src[0];
                rgba[at + 1] = src[0];
                rgba[at + 2] = src[0];
                rgba[at + 3] = 0xff;
            }
            png::ColorType::GrayscaleAlpha => {
                rgba[at] = src[0];
                rgba[at + 1] = src[0];
                rgba[at + 2] = src[0];
                rgba[at + 3] = src[1];
            }
            png::ColorType::Rgb => {
                rgba[at] = src[0];
                rgba[at + 1] = src[1];
                rgba[at + 2] = src[2];
                rgba[at + 3] = 0xff;
            }
            png::ColorType::Rgba => {
                rgba[at..at + 4].copy_from_slice(&src[..4]);
            }
        }
    }
    Ok((width, height, rgba))
}

/// Quantize RGBA8 → palette indices; alpha=0 maps to `u16::MAX` (transparent).
fn quantize_to_indices(rgba: &[u8], palette: &wad::types::WadPalette) -> Vec<u16> {
    rgba.chunks_exact(4)
        .map(|px| {
            if px[3] == 0 {
                return u16::MAX;
            }
            let colour =
                0xff00_0000 | (u32::from(px[0]) << 16) | (u32::from(px[1]) << 8) | u32::from(px[2]);
            u16::from(nearest_palette_index(colour, &palette.0))
        })
        .collect()
}

fn select(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, index: i32) {
    let ctl = ui.global::<TexEditController>();
    // Texture switch clears selection; drag re-sets it after.
    if ctl.get_current() != index {
        ctl.set_selected_patch(-1);
        shared.borrow_mut().texedit.drag = None;
    }
    ctl.set_current(index);
    let rows: Vec<TexPatchRow> = {
        let state = shared.borrow();
        let def = textures_set_ref(&state).and_then(|set| set.get(index as usize));
        match def {
            Some(def) => {
                ctl.set_tex_width(slint::format!("{}", def.width));
                ctl.set_tex_height(slint::format!("{}", def.height));
                def.patches
                    .iter()
                    .map(|p| TexPatchRow {
                        name: p.patch.as_str().into(),
                        x: slint::format!("{}", p.origin_x),
                        y: slint::format!("{}", p.origin_y),
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    };
    ctl.set_patches(model(rows));
    refresh_preview(ui, shared, index);
}

fn refresh_preview(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, index: i32) {
    let ctl = ui.global::<TexEditController>();
    let highlight = {
        let hi = ctl.get_selected_patch();
        (hi >= 0).then_some(hi as usize)
    };
    let level = ctl.get_colormap_level(); // -1 = full bright (no colormap)
    let (dims, image) = {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_assets() {
            return;
        }
        let SharedState {
            wad_data,
            assets,
            ..
        } = state;
        let wad = wad_data.as_ref().expect("ensured by ensure_assets");
        let assets = assets.as_ref().expect("ensured above");
        let colormap = (level >= 0)
            .then(|| assets.colormap(level as usize))
            .flatten();
        let Some(def) = assets.textures().get(index as usize) else {
            ctl.set_preview(slint::Image::default());
            return;
        };
        let buf = compose_texture_highlight(
            def,
            assets.imported_patches(),
            wad,
            assets.palette(),
            highlight,
            colormap.map(|c| c.as_slice()),
        );
        (
            (def.width.max(1) as f32, def.height.max(1) as f32),
            slint::Image::from_rgba8(buf),
        )
    };
    ctl.set_preview(image);
    let zoom = shared.borrow().texedit.zoom.max(1.0);
    ctl.set_preview_w(dims.0 * zoom);
    ctl.set_preview_h(dims.1 * zoom);
}
