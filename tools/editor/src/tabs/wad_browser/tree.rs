//! Source tree, project files and column interaction.

use crate::app::preferences;
use std::{
    fs,
    path::{Path, PathBuf},
};

use editor_core::load_map_ron;
use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, Pixels, ScrollStrategy, div, prelude::*, px,
    uniform_list,
};
use ui_common::widgets::{Button, ButtonStyle, Icon, IconName, Label, ListItem, prelude::*};

use super::{Column, ColumnDrag, MapSource, WadBrowser};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum NodeKey {
    Wad(usize),
    Dir(PathBuf),
    File(PathBuf),
}

#[derive(Clone)]
pub(super) enum LeafRef {
    Wad(usize),
    File(PathBuf),
}

pub(super) struct TreeNode {
    key: NodeKey,
    label: String,
    depth: usize,
    pub(super) expandable: bool,
    pub(super) expanded: bool,
    leaf: Option<LeafRef>,
}

impl WadBrowser {
    pub(super) fn rebuild_tree(&mut self) {
        let selected = self
            .selected_tree
            .and_then(|i| self.nodes.get(i))
            .map(|node| node.key.clone());
        let mut nodes = self
            .wads
            .iter()
            .enumerate()
            .map(|(index, loaded)| TreeNode {
                key: NodeKey::Wad(index),
                label: format!(
                    "{} ({})",
                    loaded.name,
                    if loaded.is_iwad { "iwad" } else { "pwad" }
                ),
                depth: 0,
                expandable: false,
                expanded: false,
                leaf: Some(LeafRef::Wad(index)),
            })
            .collect();
        let state = self.shared.borrow();
        if let Some(dir) = state.project.as_ref().and_then(|project| project.dir()) {
            append_directory(&mut nodes, dir, 0, &self.expanded);
        }
        self.nodes = nodes;
        self.selected_tree =
            selected.and_then(|key| self.nodes.iter().position(|node| node.key == key));
    }

    pub(super) fn toggle_expand(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(node) = self.nodes.get(index).filter(|node| node.expandable) else {
            return;
        };
        let key = node.key.clone();
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        self.rebuild_tree();
        cx.notify();
    }

    pub(super) fn select_tree(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(node) = self.nodes.get(index) else {
            return;
        };
        let leaf = node.leaf.clone();
        self.selected_tree = Some(index);
        self.selected_map = None;
        self.selected_lump = None;
        self.preview.clear();
        self.selected_wad = match leaf {
            Some(LeafRef::Wad(index)) => Some(index),
            _ => None,
        };
        if let Some(wad) = self.selected_wad {
            self.lumps = super::lumps::rows(&self.wads[wad].wad);
            self.filter_lumps();
        } else if let Some(LeafRef::File(path)) = leaf {
            self.show_file(&path);
        }
        self.tree_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub(super) fn activate_tree(&mut self, index: usize, cx: &mut Context<Self>) {
        self.select_tree(index, cx);
        if self.selected_map.is_some() {
            self.open_map(cx);
        }
    }

    fn show_file(&mut self, path: &Path) {
        self.preview.title = file_name(path);
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ron"))
        {
            match load_map_ron(path) {
                Ok(map) => {
                    self.selected_map = Some(MapSource::Ron(path.to_path_buf()));
                    self.preview.wireframe = Some(super::preview::Wireframe::from_map(&map));
                    return;
                }
                Err(error) => log::error!("Preview {}: {error}", path.display()),
            }
        }
        let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        self.preview.text = format!("{}\n{size} bytes", path.display());
    }

    pub(super) fn tree_column(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .p_3()
            .gap_2()
            .child(Label::new("Project"))
            .child(
                Button::new("load-pwad", "Load PWAD…")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|_, _, _, cx| Self::load_pwad(cx))),
            )
            .child(
                uniform_list(
                    "source-tree",
                    self.nodes.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| this.tree_row(index, cx)).collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .track_scroll(&self.tree_scroll),
            )
            .into_any_element()
    }

    fn tree_row(&self, index: usize, cx: &Context<Self>) -> AnyElement {
        let node = &self.nodes[index];
        let mut row = ListItem::new(("tree", index))
            .height(px(32.))
            .indent_level(node.depth)
            .indent_step_size(px(14.))
            .toggle_state(self.selected_tree == Some(index))
            .aria_label(node.label.clone())
            .start_slot(Icon::new(if matches!(node.leaf, Some(LeafRef::File(_))) {
                IconName::FileGeneric
            } else {
                IconName::Folder
            }))
            .child(Label::new(node.label.clone()).truncate());
        if node.expandable {
            row = row
                .toggle(node.expanded)
                .on_toggle(cx.listener(move |this, _, _, cx| this.toggle_expand(index, cx)));
        }
        row.on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                this.focus(window, cx);
                this.lumps_focused = false;
                if event.click_count() >= 2 {
                    this.activate_tree(index, cx);
                } else {
                    this.select_tree(index, cx);
                }
            }),
        )
        .into_any_element()
    }

    pub(super) fn splitter(&self, column: Column, cx: &Context<Self>) -> AnyElement {
        let (id, width) = match column {
            Column::Tree => ("tree-splitter", self.tree_width),
            Column::Lumps => ("lump-splitter", self.lump_width),
        };
        div()
            .id(id)
            .w(px(5.))
            .h_full()
            .flex_shrink_0()
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.column_drag = Some(ColumnDrag {
                        column,
                        start: event.position.x.into(),
                        width,
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn update_columns(&mut self, x: Pixels) -> bool {
        let Some(drag) = &self.column_drag else {
            return false;
        };
        let width = drag.width + f32::from(x) - drag.start;
        match drag.column {
            Column::Tree => self.tree_width = width.clamp(180., 640.),
            Column::Lumps => self.lump_width = width.clamp(240., 640.),
        }
        true
    }

    pub(crate) fn finish_column_drag(&mut self) {
        if self.column_drag.take().is_none() {
            return;
        }
        let mut state = self.shared.borrow_mut();
        state.prefs.browser_tree_w = self.tree_width;
        state.prefs.browser_lump_w = self.lump_width;
        if let Err(error) = preferences::save_prefs(&state.prefs) {
            log::warn!("Save browser column widths: {error}");
        }
    }
}

fn append_directory(
    nodes: &mut Vec<TreeNode>,
    dir: &Path,
    depth: usize,
    expanded: &std::collections::HashSet<NodeKey>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let is_dir = path.is_dir();
        let key = if is_dir {
            NodeKey::Dir(path.clone())
        } else {
            NodeKey::File(path.clone())
        };
        let open = expanded.contains(&key);
        nodes.push(TreeNode {
            key,
            label: entry.file_name().to_string_lossy().into_owned(),
            depth,
            expandable: is_dir,
            expanded: open,
            leaf: (!is_dir).then(|| LeafRef::File(path.clone())),
        });
        if is_dir && open {
            append_directory(nodes, &path, depth + 1, expanded);
        }
    }
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
