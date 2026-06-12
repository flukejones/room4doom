//! Project/WAD browser: wires `ProjectBrowserController`, WAD tree, lump list, content preview.

mod conversions;
mod wireframe;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::slice;

use slint::{Brush, Color, ComponentHandle as _, Model as _};
use wad::WadData;

use editor_core::{import_wad_map, load_map_ron};

use crate::SharedState;
use crate::assets::EditorAssets;
use crate::generated::{EditorWindow, ProjectBrowserController, Tabs};
use crate::gfx::GfxCache;
use crate::prefs::save_prefs;
use crate::project::{WadLoad, load_into, load_wad_file};
use crate::views::model;
use crate::views::view_project_browser::conversions::LumpKind;
use crate::views::view_project_browser::wireframe::WirePaths;
use crate::views::view_tex_browser::name_matches;
use crate::views::view_tex_edit;

/// A node's stable identity, so expand state survives a tree rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum NodeKey {
    /// A WAD by its index in the source list.
    Wad(usize),
    /// A directory under the project, by its path.
    Dir(PathBuf),
}

/// Flattened tree row + its Rust referent (click → WAD/lump/file without view round-trip).
pub(crate) struct TreeNode {
    pub key: Option<NodeKey>,
    pub label: String,
    pub depth: i32,
    pub expandable: bool,
    pub expanded: bool,
    pub container: bool,
    pub leaf: Option<LeafRef>,
}

/// What a leaf row points at; drives container/content view on select.
#[derive(Debug, Clone)]
pub(crate) enum LeafRef {
    Wad(usize),
    File(PathBuf),
}

/// Origin of the previewed map, so Edit Map loads the right way.
#[derive(Debug, Clone)]
pub(crate) enum MapSource {
    Wad(usize, String), // wad index + map name
    Ron(PathBuf),
}

/// One loaded WAD in the browser; independent of the merged editor IWAD view.
pub(crate) struct LoadedWad {
    pub name: String,
    pub is_iwad: bool,
    pub path: PathBuf,
    pub wad: WadData,
    /// Per-WAD asset + thumbnail cache; built lazily on first image preview.
    pub assets: Option<EditorAssets>,
    pub gfx: Option<GfxCache>,
}

/// Transient browser state; rebuilt on populate, expand state persists.
#[derive(Default)]
struct BrowserState {
    wads: Vec<LoadedWad>,
    nodes: Vec<TreeNode>,
    expanded: HashSet<NodeKey>,
    selected_map: Option<MapSource>,
    loaded_sig: Vec<PathBuf>, // WAD paths tree was built from; unchanged → skip rebuild
    wireframe_cache: HashMap<(usize, String), WirePaths>,
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let browser = Rc::new(RefCell::new(BrowserState::default()));
    let ctl = ui.global::<ProjectBrowserController>();

    {
        let prefs = &shared.borrow().prefs;
        ctl.set_tree_w(prefs.browser_tree_w);
        ctl.set_lump_w(prefs.browser_lump_w);
    }

    let weak = ui.as_weak();
    let s = shared.clone();
    ctl.on_widths_changed(move || {
        let Some(ui) = weak.upgrade() else { return };
        let ctl = ui.global::<ProjectBrowserController>();
        let mut state = s.borrow_mut();
        state.prefs.browser_tree_w = ctl.get_tree_w();
        state.prefs.browser_lump_w = ctl.get_lump_w();
        if let Err(e) = save_prefs(&state.prefs) {
            log::warn!("save column-width prefs: {e}");
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_populate(move || {
        if let Some(ui) = weak.upgrade() {
            populate(&ui, &s, &b);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_toggle_expand(move |row| {
        if let Some(ui) = weak.upgrade() {
            toggle_expand(&ui, &s, &b, row as usize);
        }
    });

    let weak = ui.as_weak();
    let b = browser.clone();
    ctl.on_select_tree_row(move |row| {
        if let Some(ui) = weak.upgrade() {
            select_tree_row(&ui, &b, row as usize);
        }
    });

    let weak = ui.as_weak();
    let b = browser.clone();
    ctl.on_select_lump(move |row| {
        if let Some(ui) = weak.upgrade() {
            select_lump(&ui, &b, row as usize);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_activate_tree_row(move |row| {
        if let Some(ui) = weak.upgrade() {
            activate_tree_row(&ui, &s, &b, row as usize);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_activate_lump(move |row| {
        if let Some(ui) = weak.upgrade() {
            activate_lump(&ui, &s, &b, row as usize);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_edit_map(move |_name| {
        if let Some(ui) = weak.upgrade() {
            edit_map(&ui, &s, &b);
        }
    });

    let weak = ui.as_weak();
    ctl.on_name_filter_changed(move |text| {
        if let Some(ui) = weak.upgrade() {
            apply_name_filter(&ui, text.as_str());
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let b = browser.clone();
    ctl.on_load_pwad(move || {
        if let Some(ui) = weak.upgrade() {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("WAD", &["wad", "WAD"])
                .pick_file()
            else {
                return;
            };
            if matches!(load_wad_file(&s, &path), WadLoad::Skipped) {
                return;
            }
            populate(&ui, &s, &b);
        }
    });

    push_wireframe_colours(ui, shared);
}

/// Patch `matches` in place — preserves selection and scroll position.
fn apply_name_filter(ui: &EditorWindow, text: &str) {
    let filter = text.to_uppercase();
    let rows = ui.global::<ProjectBrowserController>().get_lump_rows();
    for i in 0..rows.row_count() {
        let mut row = rows.row_data(i).expect("in range");
        let matches = name_matches(row.name.as_str(), &filter);
        if row.matches != matches {
            row.matches = matches;
            rows.set_row_data(i, row);
        }
    }
}

/// Push wireframe stroke colours from canvas style; re-run on theme change.
pub(crate) fn push_wireframe_colours(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let style = shared.borrow().app.style;
    let brush = |c: [u8; 4]| Brush::from(Color::from_argb_u8(c[3], c[0], c[1], c[2]));
    let ctl = ui.global::<ProjectBrowserController>();
    ctl.set_wf_col_one_sided(brush(style.one_sided));
    ctl.set_wf_col_two_sided(brush(style.two_sided));
    ctl.set_wf_col_special(brush(style.special));
}

/// Load previewed map into editor and switch to Map tab.
fn edit_map(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
) {
    let source = browser.borrow().selected_map.clone();
    match source {
        Some(MapSource::Wad(wad_idx, name)) => {
            let imported = browser
                .borrow()
                .wads
                .get(wad_idx)
                .map(|loaded| import_wad_map(&loaded.wad, &name));
            match imported {
                Some(Ok(map)) => {
                    if load_into(ui, shared, map, &name) {
                        ui.set_active_tab(ui.global::<Tabs>().get_map());
                    }
                }
                Some(Err(e)) => log::error!("import {name}: {e}"),
                None => {}
            }
        }
        Some(MapSource::Ron(path)) => open_ron_map(ui, shared, &path),
        None => {}
    }
}

/// Fill browser on show edge; idempotent — unchanged WAD set skips rebuild.
fn populate(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
) {
    let sig = wad_signature(shared);
    if browser.borrow().loaded_sig == sig {
        return;
    }
    ui.global::<ProjectBrowserController>().reset_browser_view();
    rebuild_wads(shared, browser);
    rebuild_tree(ui, shared, browser);
    browser.borrow_mut().loaded_sig = sig;
}

impl ProjectBrowserController<'_> {
    /// Blank the view; `rebuild_tree` repopulates `tree-rows`.
    fn reset_browser_view(&self) {
        self.set_tree_rows(model(Vec::new()));
        self.set_lump_rows(model(Vec::new()));
        self.set_tree_active(-1);
        self.set_lump_active(-1);
        self.set_container_visible(false);
        self.set_map_selected(false);
        self.set_selected_map(slint::SharedString::new());
        self.set_content_kind(LumpKind::Other.into());
        self.set_content_image(slint::Image::default());
        self.set_content_text(slint::SharedString::new());
        self.set_wf_one_sided(slint::SharedString::new());
        self.set_wf_two_sided(slint::SharedString::new());
        self.set_wf_special(slint::SharedString::new());
        self.set_name_filter(slint::SharedString::new());
        self.set_kind_filter(0);
    }
}

/// Content signature (project dir + IWAD + PWADs); dir included so project switch rebuilds.
fn wad_signature(shared: &Rc<RefCell<SharedState>>) -> Vec<PathBuf> {
    let state = shared.borrow();
    let mut sig = Vec::with_capacity(2 + state.pwads.len());
    sig.extend(
        state
            .project
            .as_ref()
            .and_then(|p| p.dir())
            .map(PathBuf::from),
    );
    sig.extend(state.iwad.clone());
    sig.extend(state.pwads.iter().cloned());
    sig
}

fn rebuild_wads(shared: &Rc<RefCell<SharedState>>, browser: &Rc<RefCell<BrowserState>>) {
    let state = shared.borrow();
    let iwad = state.iwad.clone();
    let pwads = state.pwads.clone();
    drop(state);

    let mut wads = Vec::with_capacity(1 + pwads.len());
    let mut push_wad = |path: &PathBuf, is_iwad: bool| match WadData::try_new(path) {
        Ok(wad) => wads.push(LoadedWad {
            name: file_name(path),
            is_iwad,
            path: path.clone(),
            wad,
            assets: None,
            gfx: None,
        }),
        Err(e) => log::warn!("skipping unreadable WAD {}: {e}", path.display()),
    };
    if let Some(iwad) = &iwad {
        push_wad(iwad, true);
    }
    for pwad in &pwads {
        push_wad(pwad, false);
    }
    let mut b = browser.borrow_mut();
    b.wads = wads;
    b.wireframe_cache.clear(); // indices changed
}

fn rebuild_tree(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
) {
    let state = shared.borrow();
    let project_dir = state.project.as_ref().and_then(|p| p.dir());
    let mut b = browser.borrow_mut();
    let nodes = conversions::build_tree(&b.wads, project_dir, &b.expanded);
    let rows = conversions::tree_rows(&nodes);
    b.nodes = nodes;
    ui.global::<ProjectBrowserController>().set_tree_rows(rows);
}

fn toggle_expand(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
    row: usize,
) {
    {
        let mut b = browser.borrow_mut();
        let Some(key) = b.nodes.get(row).and_then(|n| n.key.clone()) else {
            return;
        };
        if !b.expanded.remove(&key) {
            b.expanded.insert(key);
        }
    }
    rebuild_tree(ui, shared, browser);
}

/// Left-tree click: WAD → lump column; project file → content; dir → select only.
fn select_tree_row(ui: &EditorWindow, browser: &Rc<RefCell<BrowserState>>, row: usize) {
    let ctl = ui.global::<ProjectBrowserController>();
    ctl.set_tree_active(row as i32);
    ctl.set_map_selected(false);
    let leaf = browser.borrow().nodes.get(row).and_then(|n| n.leaf.clone());
    match leaf {
        Some(LeafRef::Wad(wad_idx)) => {
            let filter = ctl.get_name_filter().to_uppercase();
            let rows = conversions::lump_rows(&browser.borrow().wads, wad_idx, &filter);
            ctl.set_lump_rows(rows);
            ctl.set_container_visible(true);
            ctl.set_lump_active(-1);
        }
        Some(LeafRef::File(path)) => {
            ctl.set_container_visible(false);
            show_file(ui, browser, &path);
        }
        None => {
            ctl.set_container_visible(false);
        }
    }
}

/// Left-tree double-click: open `.ron` map; else behave like select.
fn activate_tree_row(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
    row: usize,
) {
    let leaf = browser.borrow().nodes.get(row).and_then(|n| n.leaf.clone());
    match leaf {
        Some(LeafRef::File(path)) if is_ron(&path) => open_ron_map(ui, shared, &path),
        _ => select_tree_row(ui, browser, row),
    }
}

/// Middle-column double-click: open map marker for editing; else select.
fn activate_lump(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
    row: usize,
) {
    select_lump(ui, browser, row);
    if let Some(set) = activated_texture_set(ui, browser, row) {
        view_tex_edit::open_set(ui, shared, set);
        return;
    }
    edit_selected_map(ui, shared, browser);
}

/// `TEXTURE<n>` lump → 0-based set index (`TEXTURE1` → 0); `None` for any other lump.
fn activated_texture_set(
    ui: &EditorWindow,
    browser: &Rc<RefCell<BrowserState>>,
    row: usize,
) -> Option<usize> {
    let ctl = ui.global::<ProjectBrowserController>();
    let b = browser.borrow();
    let wad_idx = match b
        .nodes
        .get(ctl.get_tree_active() as usize)
        .and_then(|n| n.leaf.as_ref())
    {
        Some(LeafRef::Wad(i)) => *i,
        _ => return None,
    };
    let loaded = b.wads.get(wad_idx)?;
    if conversions::classify(&loaded.wad, row) != LumpKind::TextureDefs {
        return None;
    }
    let name = &loaded.wad.lumps().get(row)?.name;
    name.strip_prefix("TEXTURE")?
        .parse::<usize>()
        .ok()
        .filter(|&n| n >= 1)
        .map(|n| n - 1)
}

fn edit_selected_map(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    browser: &Rc<RefCell<BrowserState>>,
) {
    if ui.global::<ProjectBrowserController>().get_map_selected() {
        edit_map(ui, shared, browser);
    }
}

fn open_ron_map(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, path: &Path) {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match load_map_ron(path) {
        Ok(map) => {
            if load_into(ui, shared, map, &name) {
                ui.set_active_tab(ui.global::<Tabs>().get_map());
            }
        }
        Err(e) => log::error!("open map {}: {e}", path.display()),
    }
}

fn is_ron(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ron"))
}

fn select_lump(ui: &EditorWindow, browser: &Rc<RefCell<BrowserState>>, row: usize) {
    let ctl = ui.global::<ProjectBrowserController>();
    ctl.set_lump_active(row as i32);
    let wad_idx = {
        let b = browser.borrow();
        match b
            .nodes
            .get(ctl.get_tree_active() as usize)
            .and_then(|n| n.leaf.as_ref())
        {
            Some(LeafRef::Wad(i)) => *i,
            _ => return,
        }
    };
    show_content(ui, browser, wad_idx, row);
}

fn show_content(
    ui: &EditorWindow,
    browser: &Rc<RefCell<BrowserState>>,
    wad_idx: usize,
    row: usize,
) {
    let ctl = ui.global::<ProjectBrowserController>();
    let mut b = browser.borrow_mut();
    let Some(loaded) = b.wads.get(wad_idx) else {
        return;
    };
    let Some(lump) = loaded.wad.lumps().get(row) else {
        return;
    };
    let name = lump.name.clone();
    let size = lump.data.len();
    let kind = conversions::classify(&loaded.wad, row);
    ctl.set_content_kind(kind.into());

    if kind == LumpKind::MapMarker {
        // wireframe preview; record source for Edit Map
        b.selected_map = Some(MapSource::Wad(wad_idx, name.clone()));
        let paths = map_wireframe(&mut b, wad_idx, &name);
        set_map_preview(&ctl, &name, &paths);
        return;
    }
    ctl.set_map_selected(false);

    if kind.is_image() {
        let Some(loaded) = b.wads.get_mut(wad_idx) else {
            return;
        };
        let LoadedWad {
            name,
            path,
            wad,
            assets,
            gfx,
            ..
        } = loaded;
        let assets = assets.get_or_insert_with(|| {
            let mut a = EditorAssets::load(slice::from_ref(path), wad, None);
            a.set_map_wad(name);
            a
        });
        let gfx = gfx.get_or_insert_with(|| GfxCache::new(assets));
        let image = conversions::image_for(gfx, assets, wad, kind, name);
        ctl.set_content_image(image.unwrap_or_default());
    } else {
        ctl.set_content_text(conversions::placeholder_text(kind, &name, size).into());
    }
}

/// Wireframe paths for `name`; builds on first request, cached by `(wad_idx, name)`.
fn map_wireframe(b: &mut BrowserState, wad_idx: usize, name: &str) -> WirePaths {
    let key = (wad_idx, name.to_owned());
    if let Some(paths) = b.wireframe_cache.get(&key) {
        return paths.clone();
    }
    let Some(loaded) = b.wads.get(wad_idx) else {
        return WirePaths::default();
    };
    let paths = match import_wad_map(&loaded.wad, name) {
        Ok(map) => wireframe::build(&map),
        Err(e) => {
            log::error!("preview {name}: {e}");
            WirePaths::default()
        }
    };
    b.wireframe_cache.insert(key, paths.clone());
    paths
}

/// Push wireframe + name + Edit Map action for WAD markers and `.ron` maps.
fn set_map_preview(ctl: &ProjectBrowserController, name: &str, paths: &WirePaths) {
    ctl.set_content_kind(LumpKind::MapMarker.into());
    ctl.set_map_selected(true);
    ctl.set_selected_map(name.into());
    push_wireframe(ctl, paths);
    ctl.set_content_text(name.into());
}

fn push_wireframe(ctl: &ProjectBrowserController, p: &WirePaths) {
    ctl.set_wf_one_sided(p.one_sided.as_str().into());
    ctl.set_wf_two_sided(p.two_sided.as_str().into());
    ctl.set_wf_special(p.special.as_str().into());
    ctl.set_wf_vb_x(p.vb_x);
    ctl.set_wf_vb_y(p.vb_y);
    ctl.set_wf_vb_w(p.vb_w);
    ctl.set_wf_vb_h(p.vb_h);
}

/// Project file content: `.ron` map → wireframe; other → placeholder.
fn show_file(ui: &EditorWindow, browser: &Rc<RefCell<BrowserState>>, path: &Path) {
    let ctl = ui.global::<ProjectBrowserController>();
    let name = file_name(path);
    if is_ron(path) {
        match load_map_ron(path) {
            Ok(map) => {
                browser.borrow_mut().selected_map = Some(MapSource::Ron(path.to_path_buf()));
                set_map_preview(&ctl, &name, &wireframe::build(&map));
                return;
            }
            Err(e) => log::error!("preview {}: {e}", path.display()),
        }
    }
    ctl.set_map_selected(false);
    ctl.set_content_kind(LumpKind::Text.into());
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    ctl.set_content_text(slint::format!(
        "{name} — file preview ({size} bytes) — TODO"
    ));
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
