//! Texture and animation commands. All mutations pass through the tab's undo stack.

use editor_core::{AnimDef, Name8, PatchPlacement, TextureDef};
use gpui::{App, Context, Focusable as _, KeyBinding, ScrollStrategy, Window};

use super::{EditAction, Input, TextureEditor};

impl TextureEditor {
    pub(crate) fn edit_action(
        &mut self,
        action: EditAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_inputs(window, cx);
        match action {
            EditAction::Undo => self.restore(false, cx),
            EditAction::Redo => self.restore(true, cx),
            EditAction::Copy => self.copy_patch(),
            EditAction::Cut => {
                self.copy_patch();
                if let Some(patch) = self.selected_patch.take() {
                    self.edit_texture(|def| {
                        if patch < def.patches.len() {
                            def.patches.remove(patch);
                        }
                    });
                    self.refresh_preview(cx);
                }
            }
            EditAction::Paste => {
                let clipboard = self.clipboard.clone();
                if !clipboard.is_empty() {
                    let mut selected = None;
                    self.edit_texture(|def| {
                        for mut patch in clipboard {
                            patch.origin_x += 8;
                            patch.origin_y += 8;
                            def.patches.push(patch);
                        }
                        selected = def.patches.len().checked_sub(1);
                    });
                    self.selected_patch = selected;
                    self.refresh_preview(cx);
                }
            }
        }
        cx.notify();
    }

    fn copy_patch(&mut self) {
        let state = self.shared.borrow();
        if let Some(patch) = self
            .current
            .and_then(|index| state.assets.as_ref()?.textures().get(index))
            .and_then(|def| def.patches.get(self.selected_patch?))
        {
            self.clipboard = vec![*patch];
        }
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let count = {
            let mut state = self.shared.borrow_mut();
            let Some(assets) = state.assets.as_mut() else {
                return;
            };
            let stepped = if redo {
                self.history.redo(assets)
            } else {
                self.history.undo(assets)
            };
            let Some(count) = stepped else { return };
            assets.refresh_texture_index();
            state.assets_dirty = true;
            count
        };
        self.inputs.clear();
        self.drafts.clear();
        self.selected_patch = None;
        self.select(
            (count > 0).then_some(self.current.unwrap_or(0).min(count.saturating_sub(1))),
            cx,
        );
    }

    pub(super) fn select(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.current != index {
            self.current = index;
            self.selected_patch = None;
            self.drag = None;
            self.inputs.clear();
            self.drafts.clear();
            if let Some(index) = index {
                self.texture_scroll
                    .scroll_to_item(index, ScrollStrategy::Nearest);
            }
        }
        self.fit_preview();
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(super) fn new_texture(&mut self, duplicate: bool, cx: &mut Context<Self>) {
        let index = {
            let mut state = self.shared.borrow_mut();
            let Some(assets) = state.assets.as_mut() else {
                return;
            };
            let def = if duplicate {
                let Some(src) = self.current.and_then(|i| assets.textures().get(i)) else {
                    return;
                };
                src.clone()
            } else {
                TextureDef {
                    name: Name8::new("NEWTEX").expect("known-valid name"),
                    width: 128,
                    height: 128,
                    patches: Vec::new(),
                }
            };
            self.history.record(assets);
            let set = assets.textures_vec_mut();
            set.push(def);
            let index = set.len() - 1;
            assets.refresh_texture_index();
            state.assets_dirty = true;
            index
        };
        self.select(Some(index), cx);
    }

    pub(super) fn delete_texture(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.current else { return };
        let count = {
            let mut state = self.shared.borrow_mut();
            let Some(assets) = state.assets.as_mut() else {
                return;
            };
            if index >= assets.textures().len() {
                return;
            }
            self.history.record(assets);
            assets.textures_vec_mut().remove(index);
            assets.refresh_texture_index();
            let count = assets.textures().len();
            state.assets_dirty = true;
            count
        };
        self.current = None;
        self.inputs.clear();
        self.drafts.clear();
        self.select(
            (count > 0).then_some(index.min(count.saturating_sub(1))),
            cx,
        );
    }

    pub(super) fn edit_texture(&mut self, edit: impl FnOnce(&mut TextureDef)) {
        let Some(index) = self.current else { return };
        let mut state = self.shared.borrow_mut();
        let Some(assets) = state.assets.as_mut() else {
            return;
        };
        let Some(old) = assets.textures().get(index) else {
            return;
        };
        let mut changed = old.clone();
        edit(&mut changed);
        if &changed == old {
            return;
        }
        self.history.record(assets);
        if let Some(def) = assets.texture_mut(index) {
            *def = changed;
        }
        assets.refresh_texture_index();
        state.assets_dirty = true;
    }

    pub(crate) fn apply_animations(&mut self, definitions: &[AnimDef], cx: &mut Context<Self>) {
        let mut state = self.shared.borrow_mut();
        let Some(assets) = state.assets.as_mut() else {
            return;
        };
        if definitions == assets.animations() {
            return;
        }
        self.history.record(assets);
        assets.replace_animations(definitions.to_vec());
        state.assets_dirty = true;
        cx.notify();
    }

    pub(super) fn add_patch(&mut self, cx: &mut Context<Self>) {
        let Ok(name) = Name8::new(self.new_patch.trim()) else {
            self.status = "Invalid patch name".into();
            cx.notify();
            return;
        };
        self.edit_texture(|def| {
            def.patches.push(PatchPlacement {
                origin_x: 0,
                origin_y: 0,
                patch: name,
                step_dir: 1,
                colormap: 0,
            });
        });
        self.refresh_preview(cx);
    }

    pub(super) fn delete_patch(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_texture(|def| {
            if index < def.patches.len() {
                def.patches.remove(index);
            }
        });
        self.selected_patch = None;
        self.inputs
            .retain(|key, _| !matches!(key, Input::PatchX(_) | Input::PatchY(_)));
        self.drafts
            .retain(|key, _| !matches!(key, Input::PatchX(_) | Input::PatchY(_)));
        self.refresh_preview(cx);
    }

    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.inputs
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(crate) fn flush_inputs(&mut self, _: &Window, cx: &mut Context<Self>) {
        let changed: Vec<_> = self.drafts.drain().collect();
        for (key, value) in changed {
            self.commit_field(key, value, cx);
        }
    }

    pub(super) fn commit_field(&mut self, key: Input, value: String, cx: &mut Context<Self>) {
        match key {
            Input::NewPatch => self.new_patch = value,
            Input::Width | Input::Height => {
                if let Ok(value) = value.trim().parse::<i32>() {
                    self.edit_texture(|def| match key {
                        Input::Width => def.width = value.clamp(1, 4096),
                        _ => def.height = value.clamp(1, 4096),
                    });
                    self.fit_preview();
                    self.refresh_preview(cx);
                }
            }
            Input::PatchX(index) | Input::PatchY(index) => {
                if let Ok(value) = value.trim().parse() {
                    self.edit_texture(|def| {
                        if let Some(patch) = def.patches.get_mut(index) {
                            match key {
                                Input::PatchX(_) => patch.origin_x = value,
                                _ => patch.origin_y = value,
                            }
                        }
                    });
                    self.refresh_preview(cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn nudge(&mut self, action: &Nudge, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_input_focused(window, cx) {
            cx.propagate();
            return;
        }
        if let Some(patch) = self.selected_patch {
            let Nudge {
                x,
                y,
            } = *action;
            self.edit_texture(|def| {
                if let Some(patch) = def.patches.get_mut(patch) {
                    patch.origin_x += x;
                    patch.origin_y += y;
                }
            });
            self.refresh_preview(cx);
        } else {
            let count = self
                .shared
                .borrow()
                .assets
                .as_ref()
                .map_or(0, |a| a.textures().len());
            let current = self.current.unwrap_or(0);
            let next = match action.y {
                y if y < 0 => current.saturating_sub(1),
                y if y > 0 => (current + 1).min(count.saturating_sub(1)),
                _ => return,
            };
            self.select((count > 0).then_some(next), cx);
        }
    }
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = texture_editor, no_json)]
pub(super) struct Nudge {
    x: i32,
    y: i32,
}

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys(
        [
            ("left", -1, 0),
            ("right", 1, 0),
            ("up", 0, -1),
            ("down", 0, 1),
            ("shift-left", -10, 0),
            ("shift-right", 10, 0),
            ("shift-up", 0, -10),
            ("shift-down", 0, 10),
        ]
        .map(|(key, x, y)| {
            KeyBinding::new(
                key,
                Nudge {
                    x,
                    y,
                },
                Some("TextureEditor"),
            )
        }),
    );
}
