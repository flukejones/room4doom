//! Texture editing as a persistent native view. Asset data belongs to the session;
//! selection, inputs, undo and preview interaction belong to this tab.

mod actions;
pub(crate) use actions::bind_keys;
mod import;
mod preview;
mod view;

#[cfg(test)]
mod tests;

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use editor_core::PatchPlacement;
use gpui::{
    Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, RenderImage,
    UniformListScrollHandle, Window,
};

use crate::{app::session::EditorSession, assets::texture::TextureHistory};

pub(crate) enum TextureEditorEvent {
    Animations,
    BrowsePatches,
    SaveProject,
}

#[derive(Clone, Copy)]
pub(crate) enum EditAction {
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Input {
    Width,
    Height,
    PatchX(usize),
    PatchY(usize),
    NewPatch,
}

struct PatchDrag {
    patch: usize,
    grab: [i32; 2],
    recorded: bool,
}

pub(crate) struct TextureEditor {
    shared: Rc<RefCell<EditorSession>>,
    focus: FocusHandle,
    current: Option<usize>,
    texture_scroll: UniformListScrollHandle,
    selected_patch: Option<usize>,
    history: TextureHistory,
    clipboard: Vec<PatchPlacement>,
    drag: Option<PatchDrag>,
    zoom: f32,
    preview_bounds: Bounds<Pixels>,
    preview_size: [f32; 2],
    preview: Option<Arc<RenderImage>>,
    retired_images: Vec<Arc<RenderImage>>,
    colormap: Option<usize>,
    inputs: HashMap<Input, Entity<ui_common::TextInput>>,
    drafts: HashMap<Input, String>,
    new_patch: String,
    status: String,
    group: Option<usize>,
}

impl EventEmitter<TextureEditorEvent> for TextureEditor {}

impl TextureEditor {
    pub(crate) fn new(shared: Rc<RefCell<EditorSession>>, cx: &Context<Self>) -> Self {
        Self {
            shared,
            focus: cx.focus_handle(),
            current: None,
            texture_scroll: UniformListScrollHandle::new(),
            selected_patch: None,
            history: TextureHistory::default(),
            clipboard: Vec::new(),
            drag: None,
            zoom: 1.0,
            preview_bounds: Bounds::default(),
            preview_size: [0.0; 2],
            preview: None,
            retired_images: Vec::new(),
            colormap: None,
            inputs: HashMap::new(),
            drafts: HashMap::new(),
            new_patch: String::new(),
            status: String::new(),
            group: None,
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(window, cx);
        self.focus(window, cx);
    }

    /// A new project or WAD replaces the asset working set and its edit history.
    pub(crate) fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.group = None;
        self.current = None;
        self.selected_patch = None;
        self.history.clear();
        self.clipboard.clear();
        self.drag = None;
        self.inputs.clear();
        self.drafts.clear();
        self.refresh(window, cx);
    }

    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = {
            let mut state = self.shared.borrow_mut();
            state.ensure_assets();
            state
                .assets
                .as_ref()
                .map(|assets| (assets.active_group(), assets.textures().len()))
        };
        let Some((group, count)) = selection else {
            self.current = None;
            if let Some(previous) = self.preview.take() {
                self.retired_images.push(previous);
            }
            self.retire_previews(window);
            cx.notify();
            return;
        };
        if self.group != Some(group) {
            self.group = Some(group);
            self.history.clear();
            self.current = None;
            self.inputs.clear();
            self.drafts.clear();
        }
        let next = self.current.unwrap_or(0).min(count.saturating_sub(1));
        self.select((count > 0).then_some(next), cx);
        self.retire_previews(window);
    }

    pub(crate) fn open_set(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_inputs(window, cx);
        {
            let mut state = self.shared.borrow_mut();
            if !state.ensure_assets() {
                return;
            }
            if let Some(assets) = state.assets.as_mut() {
                assets.set_active_group(index);
            }
        }
        self.activate(window, cx);
    }

    pub(crate) fn open_source_set(
        &mut self,
        wad_name: &str,
        lump: &editor_core::Name8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = {
            let mut state = self.shared.borrow_mut();
            state.ensure_assets();
            state
                .assets
                .as_ref()
                .and_then(|assets| assets.texture_group_index(wad_name, lump))
        };
        if let Some(index) = index {
            self.open_set(index, window, cx);
        }
    }

    pub(crate) fn choose_patch(&mut self, name: String, _: &mut Window, cx: &mut Context<Self>) {
        self.new_patch = name;
        self.add_patch(cx);
    }

    pub(crate) fn cancel_drag(&mut self, cx: &mut Context<Self>) {
        self.drag = None;
        cx.notify();
    }
}
