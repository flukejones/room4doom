//! Persistent native tabs and typed application-level event routing.
use super::{
    appearance::EditorAppearance,
    commands::{self, AppCommand, AppEffect},
    host::HostTasks,
    session::{Damage, EditorSession},
};
use crate::components::forms::flush_input_registry;
use crate::dialogs::{
    DialogRequest, DialogsEvent, DialogsView, asset_picker::actions::TexBrowseTarget,
};
use crate::tabs::{
    map_editor::{MapEditor, MapEvent, MapRequest},
    texture_editor::{TextureEditor, TextureEditorEvent},
    wad_browser::{WadBrowser, WadBrowserEvent},
};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, KeyDownEvent, MouseButton,
    MouseMoveEvent, Render, Subscription, Window, div, prelude::*,
};
use std::{cell::RefCell, rc::Rc};
use ui_common::widgets::{Clickable as _, Label, Tab, TabBar, TabPosition, Toggleable as _};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EditorTab {
    #[default]
    Project,
    Map,
    Textures,
}

pub(crate) struct EditorShell {
    pub(crate) appearance: EditorAppearance,
    pub(crate) shared: Rc<RefCell<EditorSession>>,
    pub(crate) map: Entity<MapEditor>,
    pub(crate) dialogs: Entity<DialogsView>,
    pub(crate) wad: Entity<WadBrowser>,
    pub(crate) textures: Entity<TextureEditor>,
    pub(crate) native: HostTasks,
    pub(crate) focus: FocusHandle,
    pub(crate) selected_tab: EditorTab,
    pub(crate) job_status: String,
    _subscriptions: Vec<Subscription>,
    active_tab: Option<EditorTab>,
    pending_dialog: Option<DialogRequest>,
    patch_choice: Option<String>,
    texture_set: Option<(String, editor_core::Name8)>,
    resource_generation: u64,
    document_generation: u64,
}
impl EditorShell {
    pub(crate) fn new(
        appearance: EditorAppearance,
        shared: Rc<RefCell<EditorSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let map = cx.new(|cx| MapEditor::new(shared.clone(), window, cx));
        let dialogs = cx.new(|cx| DialogsView::new(shared.clone(), cx));
        let wad = cx.new(|cx| WadBrowser::new(shared.clone(), window, cx));
        let textures = cx.new(|cx| TextureEditor::new(shared.clone(), cx));
        let mut subscriptions = vec![cx.observe(&map, |_, _, cx| cx.notify())];
        subscriptions.push(cx.observe(&textures, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&dialogs, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe(&map, |this, _, event, cx| {
            match event {
                MapEvent::Request(MapRequest::Dialog(request)) => {
                    this.pending_dialog = Some(request.clone());
                }
                MapEvent::Request(MapRequest::Launch) => {
                    this.dispatch_command(AppCommand::Launch, cx);
                }
            }
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&dialogs, |this, _, event, cx| {
            this.dialog_event(event, cx);
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&wad, |this, _, event, cx| {
            match event {
                WadBrowserEvent::OpenMap {
                    map,
                    name,
                } => this.dispatch_command(
                    AppCommand::LoadMap {
                        map: map.clone(),
                        name: name.clone(),
                    },
                    cx,
                ),
                WadBrowserEvent::OpenTextureSet {
                    wad_name,
                    lump,
                } => {
                    this.texture_set = Some((wad_name.clone(), *lump));
                    this.selected_tab = EditorTab::Textures;
                }
                WadBrowserEvent::AddWad(path) => {
                    this.dispatch_command(AppCommand::LoadWad(path.clone()), cx);
                }
            }
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&textures, |this, _, event, cx| {
            match event {
                TextureEditorEvent::BrowsePatches => {
                    this.pending_dialog = Some(DialogRequest::AssetPicker {
                        target: TexBrowseTarget::Patch,
                        current: None,
                    });
                }
                TextureEditorEvent::Animations => {
                    this.pending_dialog = Some(DialogRequest::Animations);
                }
                TextureEditorEvent::SaveProject => this.dispatch_command(AppCommand::Save, cx),
            }
            cx.notify();
        }));
        let state = shared.borrow();
        let resource_generation = state.resource_generation;
        let document_generation = state.document_generation;
        let selected_tab = if state.map_editor.map.is_some() {
            EditorTab::Map
        } else {
            EditorTab::Project
        };
        drop(state);
        Self {
            appearance,
            shared,
            map,
            dialogs,
            wad,
            textures,
            native: HostTasks::default(),
            focus: cx.focus_handle(),
            selected_tab,
            job_status: String::new(),
            _subscriptions: subscriptions,
            active_tab: None,
            pending_dialog: None,
            patch_choice: None,
            texture_set: None,
            resource_generation,
            document_generation,
        }
    }
    fn dialog_event(&mut self, event: &DialogsEvent, cx: &mut Context<Self>) {
        match event {
            DialogsEvent::Appearance(appearance) => self.update_system_appearance(*appearance, cx),
            DialogsEvent::Quit => {
                self.dialogs
                    .update(cx, |dialogs, cx| dialogs.close_popup(cx));
                self.dispatch_command(AppCommand::Quit, cx);
            }
            DialogsEvent::Animations(definitions) => {
                self.textures.update(cx, |textures, cx| {
                    textures.apply_animations(definitions, cx);
                });
            }
            DialogsEvent::Opened => {
                self.cancel_canvas_gesture(cx);
                self.cancel_texture_drag(cx);
                self.finish_chrome_drag(cx);
                self.map.update(cx, |map, _| map.modal_active = true);
            }
            DialogsEvent::Closed => {
                self.map.update(cx, |map, _| map.modal_active = false);
                self.active_tab = None;
            }
            DialogsEvent::Damage(damage) => self.map.update(cx, |map, cx| map.refresh(*damage, cx)),
            DialogsEvent::PreferencesChanged => {
                self.appearance.refresh(&self.shared);
                self.map
                    .update(cx, |map, cx| map.refresh(Damage::Restyle, cx));
            }
            DialogsEvent::ProjectSettingsChanged => self
                .map
                .update(cx, |map, cx| map.refresh(Damage::Edited, cx)),
            DialogsEvent::ChooseMap(name) => {
                self.dispatch_command(AppCommand::ChooseMap(name.clone()), cx);
            }
            DialogsEvent::NewMap(name) => {
                self.dispatch_command(AppCommand::NewMap(name.clone()), cx);
            }
            DialogsEvent::BuildBsp {
                interval_ms,
                keep_all,
            } => self.dispatch_command(
                AppCommand::BuildBsp {
                    interval_ms: *interval_ms,
                    keep_all: *keep_all,
                },
                cx,
            ),
            DialogsEvent::AssetPicked {
                target: TexBrowseTarget::Patch,
                name,
            } => self.patch_choice = Some(name.clone()),
            DialogsEvent::AssetPicked {
                target,
                name,
            } => self
                .map
                .update(cx, |map, cx| map.apply_asset_pick(*target, name, cx)),
        }
    }
    /// Native dialogs may pump events; run domain commands outside GPUI entity borrows.
    pub(crate) fn dispatch_command(&self, command: AppCommand, cx: &Context<Self>) {
        let shared = self.shared.clone();
        let identity = shared.borrow().identity();
        cx.spawn(async move |shell, cx| {
            if command.belongs_to_document() && identity != shared.borrow().identity() {
                return;
            }
            let effect = commands::run(&shared, command);
            _ = shell.update(cx, |this, cx| this.apply_effect(effect, cx));
        })
        .detach();
    }
    fn apply_effect(&mut self, effect: AppEffect, cx: &mut Context<Self>) {
        if let Some(status) = effect.status {
            self.job_status = status;
        }
        if effect.quit {
            cx.quit();
            return;
        }
        if effect.choose_map {
            self.pending_dialog = Some(DialogRequest::ChooseMap);
        }
        if effect.damage != Damage::None {
            let state = self.shared.borrow();
            if state.document_generation != self.document_generation {
                self.selected_tab = if state.map_editor.map.is_some() {
                    EditorTab::Map
                } else {
                    EditorTab::Project
                };
            }
            drop(state);
            self.map
                .update(cx, |map, cx| map.refresh(effect.damage, cx));
        }
        cx.notify();
    }
    pub(crate) fn queue_dialog(&mut self, request: DialogRequest) {
        self.pending_dialog = Some(request);
    }
    pub(crate) fn flush_inputs(&self, window: &mut Window, cx: &mut App) {
        // Blur listeners update their entity, so release its read borrow before committing.
        let map_inputs = self.map.read(cx).input_registry();
        let dialog_inputs = self.dialogs.read(cx).input_registry();
        flush_input_registry(&map_inputs, window, cx);
        self.map.update(cx, |map, cx| {
            map.flush_inspector(window, cx);
        });
        if let Some(dialog) = self.dialogs.read(cx).native_window()
            && dialog != window.window_handle()
        {
            dialog
                .update(cx, |_, window, cx| {
                    flush_input_registry(&dialog_inputs, window, cx)
                })
                .ok();
        } else {
            flush_input_registry(&dialog_inputs, window, cx);
        }
        self.textures
            .update(cx, |tab, cx| tab.flush_inputs(window, cx));
    }
    pub(crate) fn focus_commands(&self, window: &mut Window, cx: &mut App) {
        if let Some(dialog) = self.dialogs.read(cx).native_window() {
            if dialog == window.window_handle() {
                self.dialogs
                    .update(cx, |dialogs, cx| dialogs.focus(window, cx));
            } else {
                let dialogs = self.dialogs.clone();
                dialog
                    .update(cx, |_, window, cx| {
                        window.activate_window();
                        dialogs.update(cx, |dialogs, cx| dialogs.focus(window, cx));
                    })
                    .ok();
            }
        } else {
            window.focus(&self.focus, cx);
        }
    }
    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.dialogs.read(cx).text_input_focused(window, cx)
            || self.map.read(cx).text_input_focused(window, cx)
            || self.textures.read(cx).text_input_focused(window, cx)
    }
    pub(crate) fn cancel_canvas_gesture(&self, cx: &mut App) {
        self.map.update(cx, |map, cx| map.cancel_canvas_gesture(cx));
    }
    pub(crate) fn cancel_texture_drag(&self, cx: &mut App) {
        self.textures.update(cx, |tab, cx| tab.cancel_drag(cx));
    }
    pub(crate) fn finish_chrome_drag(&self, cx: &mut App) {
        self.map.update(cx, |map, _| map.finish_chrome_drag());
        self.wad.update(cx, |wad, _| wad.finish_column_drag());
    }
    pub(crate) fn switch_tab(
        &mut self,
        tab: EditorTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_inputs(window, cx);
        self.selected_tab = tab;
        self.activate_tab(window, cx);
        cx.notify();
    }
    fn activate_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialogs.read(cx).active_popup().is_some()
            || self.active_tab == Some(self.selected_tab)
        {
            return;
        }
        self.flush_inputs(window, cx);
        self.cancel_canvas_gesture(cx);
        self.cancel_texture_drag(cx);
        self.finish_chrome_drag(cx);
        match self.selected_tab {
            EditorTab::Map => self.map.update(cx, |map, cx| map.focus(window, cx)),
            EditorTab::Textures => self.textures.update(cx, |tab, cx| tab.activate(window, cx)),
            EditorTab::Project => self.wad.update(cx, |tab, cx| tab.activate(window, cx)),
        }
        self.active_tab = Some(self.selected_tab);
    }
    fn tabs(&self, cx: &Context<Self>) -> TabBar {
        let active = self.selected_tab;
        let state = self.shared.borrow();
        let title = if state.map_editor.map.is_some() {
            format!("Map - {}", state.map_editor.map_name)
        } else {
            "Map".to_owned()
        };
        TabBar::new("editor-tabs").children(
            [
                (EditorTab::Project, "Project".to_owned()),
                (EditorTab::Map, title),
                (EditorTab::Textures, "Texture Editor".to_owned()),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (tab, title))| {
                Tab::new(("editor-tab", index))
                    .position(match index {
                        0 => TabPosition::First,
                        2 => TabPosition::Last,
                        _ => TabPosition::Middle,
                    })
                    .toggle_state(tab == active)
                    .child(Label::new(title))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.switch_tab(tab, window, cx)),
                    )
            }),
        )
    }
    fn update_features(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (resources, document) = {
            let state = self.shared.borrow();
            (state.resource_generation, state.document_generation)
        };
        if document != self.document_generation {
            self.dialogs
                .update(cx, |dialogs, cx| dialogs.close_popup(cx));
            self.document_generation = document;
        }
        if resources != self.resource_generation {
            self.textures.update(cx, |tab, cx| tab.reset(window, cx));
            self.resource_generation = resources;
        }
        if let Some(request) = self.pending_dialog.take() {
            self.flush_inputs(window, cx);
            self.dialogs
                .update(cx, |dialogs, cx| dialogs.open(request, window, cx));
        }
        self.activate_tab(window, cx);
        if let Some((wad_name, lump)) = self.texture_set.take() {
            self.textures.update(cx, |tab, cx| {
                tab.open_source_set(&wad_name, &lump, window, cx);
            });
        }
        if let Some(name) = self.patch_choice.take() {
            self.textures
                .update(cx, |tab, cx| tab.choose_patch(name, window, cx));
        }
        self.wad.update(cx, |tab, cx| tab.refresh(cx));
    }
}
impl Render for EditorShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.native_tick(window, cx);
        self.update_features(window, cx);
        let mut background = ui_common::color(self.appearance.chrome.window_background);
        background.a = self.appearance.glass_alpha;
        let title = {
            let state = self.shared.borrow();
            format!(
                "editor{}{}",
                state
                    .project_name()
                    .map(|name| format!(" - {name}"))
                    .unwrap_or_default(),
                if state.assets_dirty || state.map_editor.dirty {
                    " *"
                } else {
                    ""
                }
            )
        };
        window.set_window_title(&title);
        let body: AnyElement = match self.selected_tab {
            EditorTab::Map => self.map.clone().into_any_element(),
            EditorTab::Textures => self.textures.clone().into_any_element(),
            EditorTab::Project => self.wad.clone().into_any_element(),
        };
        let menu = (!cfg!(target_os = "macos")).then(|| self.menu_bar(window, cx));
        self.bind_native_actions(div(), cx)
            .id("editor-root")
            .track_focus(&self.focus)
            .tab_group()
            .relative()
            .size_full()
            .bg(background)
            .text_sm()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this
                    .map
                    .update(cx, |map, _| map.update_chrome_drag(event.position))
                {
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.map.update(cx, |map, _| map.finish_chrome_drag());
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab"
                    && this.selected_tab == EditorTab::Map
                    && this.dialogs.read(cx).active_popup().is_none()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                {
                    this.flush_inputs(window, cx);
                    this.map.update(cx, |map, cx| {
                        map.tab_control(event.keystroke.modifiers.shift, window, cx);
                    });
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(ui_common::titlebar(title, cx).bg(background))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .border_b_1()
                            .children(menu)
                            .child(self.tabs(cx)),
                    )
                    .child(div().flex_1().min_h_0().child(body))
                    .children((!self.job_status.is_empty()).then(|| {
                        div()
                            .px_3()
                            .py_1()
                            .border_t_1()
                            .child(Label::new(self.job_status.clone()))
                    })),
            )
    }
}
