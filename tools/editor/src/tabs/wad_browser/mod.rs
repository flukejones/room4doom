//! Project resources, WAD contents and previews, owned by one persistent GPUI tab.

mod lumps;
mod preview;
mod tree;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use editor_core::{EditorMap, Name8, import_wad_map, load_map_ron};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, UniformListScrollHandle,
    Window, div, prelude::*, px,
};
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Button, ButtonStyle, Label, prelude::*};

use crate::{EditorSession, assets::EditorAssets, gfx::GfxCache};
use lumps::{LumpKind, LumpRow};
use preview::Preview;
use tree::{NodeKey, TreeNode};

/// Requests that cross the tab boundary. The shell owns document guards and tab changes.
pub(crate) enum WadBrowserEvent {
    OpenMap { map: Box<EditorMap>, name: String },
    OpenTextureSet { wad_name: String, lump: Name8 },
    AddWad(PathBuf),
}

impl EventEmitter<WadBrowserEvent> for WadBrowser {}

struct LoadedWad {
    name: String,
    is_iwad: bool,
    path: PathBuf,
    wad: wad::WadData,
    assets: Option<EditorAssets>,
    gfx: Option<GfxCache>,
}

#[derive(Clone)]
enum MapSource {
    Wad(usize, String),
    Ron(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Tree,
    Lumps,
}

struct ColumnDrag {
    column: Column,
    start: f32,
    width: f32,
}

#[derive(PartialEq, Eq)]
struct SourceKey {
    generation: u64,
    paths: Vec<PathBuf>,
    project_dir: Option<PathBuf>,
    map_count: usize,
}

pub(crate) struct WadBrowser {
    shared: Rc<RefCell<EditorSession>>,
    focus: FocusHandle,
    wads: Vec<LoadedWad>,
    loaded_signature: Option<SourceKey>,
    nodes: Vec<TreeNode>,
    expanded: HashSet<NodeKey>,
    selected_tree: Option<usize>,
    selected_wad: Option<usize>,
    selected_lump: Option<usize>,
    lumps_focused: bool,
    lumps: Vec<LumpRow>,
    visible_lumps: Vec<usize>,
    kind_filter: Option<LumpKind>,
    name_filter: String,
    filter_input: Entity<ui_common::TextInput>,
    kind_menu: Option<Entity<ui_common::widgets::ContextMenu>>,
    tree_scroll: UniformListScrollHandle,
    lump_scroll: UniformListScrollHandle,
    tree_width: f32,
    lump_width: f32,
    column_drag: Option<ColumnDrag>,
    preview: Preview,
    selected_map: Option<MapSource>,
    wireframes: HashMap<(usize, String), Arc<preview::Wireframe>>,
}

impl WadBrowser {
    pub(crate) fn new(
        shared: Rc<RefCell<EditorSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let weak = cx.weak_entity();
        let filter_input = cx.new(|cx| {
            ui_common::TextInput::new(String::new(), window, cx).on_change(move |text, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.name_filter = text.to_uppercase();
                    this.filter_lumps();
                    cx.notify();
                });
            })
        });
        let (tree_width, lump_width) = {
            let state = shared.borrow();
            (state.prefs.browser_tree_w, state.prefs.browser_lump_w)
        };
        let mut browser = Self {
            shared,
            focus: cx.focus_handle(),
            wads: Vec::new(),
            loaded_signature: None,
            nodes: Vec::new(),
            expanded: HashSet::new(),
            selected_tree: None,
            selected_wad: None,
            selected_lump: None,
            lumps_focused: false,
            lumps: Vec::new(),
            visible_lumps: Vec::new(),
            kind_filter: None,
            name_filter: String::new(),
            filter_input,
            kind_menu: None,
            tree_scroll: UniformListScrollHandle::new(),
            lump_scroll: UniformListScrollHandle::new(),
            tree_width: tree_width.clamp(180., 640.),
            lump_width: lump_width.clamp(240., 640.),
            column_drag: None,
            preview: Preview::default(),
            selected_map: None,
            wireframes: HashMap::new(),
        };
        browser.refresh(cx);
        browser
    }

    /// Reload when sources change or the application replaces resources at the same paths.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_sources(cx);
    }

    /// Refresh directory contents on activation without discarding WAD decode caches.
    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.refresh_sources(cx) {
            self.rebuild_tree();
        }
        self.focus(window, cx);
        cx.notify();
    }

    fn refresh_sources(&mut self, cx: &mut Context<Self>) -> bool {
        let (generation, project_dir, map_count, iwad, pwads) = {
            let state = self.shared.borrow();
            let project_dir = state.project.as_ref().and_then(|project| project.dir());
            let map_count = state
                .project
                .as_ref()
                .map_or(0, |project| project.maps.len());
            if self.loaded_signature.as_ref().is_some_and(|old| {
                old.generation == state.resource_generation
                    && old.project_dir.as_deref() == project_dir
                    && old.map_count == map_count
                    && old.paths.iter().eq(state.iwad.iter().chain(&state.pwads))
            }) {
                return false;
            }
            (
                state.resource_generation,
                project_dir.map(PathBuf::from),
                map_count,
                state.iwad.clone(),
                state.pwads.clone(),
            )
        };
        let paths: Vec<_> = iwad.iter().chain(&pwads).cloned().collect();
        let signature = SourceKey {
            generation,
            paths,
            project_dir,
            map_count,
        };
        let wads_changed = self.loaded_signature.as_ref().is_none_or(|old| {
            old.generation != signature.generation || old.paths != signature.paths
        });
        self.loaded_signature = Some(signature);
        if !wads_changed {
            self.rebuild_tree();
            cx.notify();
            return true;
        }
        self.wads.clear();
        self.wireframes.clear();
        for (path, is_iwad) in iwad
            .into_iter()
            .map(|p| (p, true))
            .chain(pwads.into_iter().map(|p| (p, false)))
        {
            match wad::WadData::try_new(&path) {
                Ok(wad) => self.wads.push(LoadedWad {
                    name: tree::file_name(&path),
                    is_iwad,
                    path,
                    wad,
                    assets: None,
                    gfx: None,
                }),
                Err(error) => log::warn!("Skipping unreadable WAD {}: {error}", path.display()),
            }
        }
        self.selected_tree = None;
        self.selected_wad = None;
        self.selected_lump = None;
        self.selected_map = None;
        self.lumps.clear();
        self.visible_lumps.clear();
        self.preview.clear();
        self.rebuild_tree();
        cx.notify();
        true
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    fn open_map(&self, cx: &mut Context<Self>) {
        let Some(source) = &self.selected_map else {
            return;
        };
        let (name, result) = match source {
            MapSource::Wad(index, name) => {
                let Some(loaded) = self.wads.get(*index) else {
                    return;
                };
                (
                    name.clone(),
                    import_wad_map(&loaded.wad, name).map_err(|error| error.to_string()),
                )
            }
            MapSource::Ron(path) => (
                path.file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                load_map_ron(path).map_err(|error| error.to_string()),
            ),
        };
        match result {
            Ok(map) => cx.emit(WadBrowserEvent::OpenMap {
                map: Box::new(map),
                name,
            }),
            Err(error) => log::error!("Open map {name}: {error}"),
        }
    }

    fn load_pwad(cx: &Context<Self>) {
        // Native dialogs can pump events; open after releasing the tab's GPUI borrow.
        cx.spawn(async move |browser, cx| {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("WAD", &["wad", "WAD"])
                .pick_file()
            {
                let _ = browser.update(cx, |_, cx| cx.emit(WadBrowserEvent::AddWad(path)));
            }
        })
        .detach();
    }

    fn handle_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if self.lumps_focused {
            if key == "left" {
                self.lumps_focused = false;
            } else if key == "enter" {
                if let Some(index) = self.selected_lump {
                    self.activate_lump(index, cx);
                }
            } else {
                let current = self
                    .selected_lump
                    .and_then(|selected| self.visible_lumps.iter().position(|&i| i == selected));
                let Some(next) = navigate(key, current, self.visible_lumps.len()) else {
                    return false;
                };
                if let Some(&index) = self.visible_lumps.get(next) {
                    self.select_lump(index, cx);
                }
            }
        } else if key == "enter" {
            if let Some(index) = self.selected_tree {
                self.activate_tree(index, cx);
            }
        } else if matches!(key, "left" | "right") {
            if let Some(index) = self.selected_tree {
                let node = &self.nodes[index];
                if node.expandable && node.expanded == (key == "left") {
                    self.toggle_expand(index, cx);
                } else if key == "right" && self.selected_wad.is_some() {
                    self.lumps_focused = true;
                }
            }
        } else {
            let Some(next) = navigate(key, self.selected_tree, self.nodes.len()) else {
                return false;
            };
            if next < self.nodes.len() {
                self.select_tree(next, cx);
            }
        }
        cx.notify();
        true
    }
}

impl Focusable for WadBrowser {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WadBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.preview.retire_images(window);
        let border = cx.theme().colors().border;
        let tree = self.tree_column(cx);
        let mut body = div()
            .flex()
            .size_full()
            .min_h_0()
            .p_3()
            .child(
                div()
                    .w(px(self.tree_width))
                    .flex_shrink_0()
                    .h_full()
                    .border_1()
                    .rounded_md()
                    .border_color(border)
                    .child(tree),
            )
            .child(self.splitter(Column::Tree, cx));
        if self.selected_wad.is_some() {
            let lumps = self.lump_column(window, cx);
            body = body
                .child(
                    div()
                        .w(px(self.lump_width))
                        .flex_shrink_0()
                        .h_full()
                        .border_1()
                        .rounded_md()
                        .border_color(border)
                        .child(lumps),
                )
                .child(self.splitter(Column::Lumps, cx));
        }
        let mut preview = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .border_1()
            .rounded_md()
            .border_color(border)
            .p_3()
            .gap_2();
        if !self.preview.title.is_empty() {
            preview = preview.child(Label::new(self.preview.title.clone()));
        }
        if self.selected_map.is_some() {
            preview = preview.child(
                Button::new("edit-map", "Edit Map")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| this.open_map(cx))),
            );
        }
        preview = preview.child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(self.render_preview(cx)),
        );
        body.child(preview)
            .id("wad-browser")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this
                    .filter_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                {
                    return;
                }
                if this.handle_key(&event.keystroke.key, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.update_columns(event.position.x) {
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.finish_column_drag()),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.finish_column_drag()),
            )
    }
}

fn navigate(key: &str, current: Option<usize>, len: usize) -> Option<usize> {
    match key {
        "up" => Some(current.unwrap_or(0).saturating_sub(1)),
        "down" => Some(current.map_or(0, |i| (i + 1).min(len.saturating_sub(1)))),
        "home" => Some(0),
        "end" => Some(len.saturating_sub(1)),
        _ => None,
    }
}
