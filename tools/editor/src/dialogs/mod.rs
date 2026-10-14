//! Persistent native modal host with feature-owned draft values.

mod animations;
pub(crate) mod asset_picker;
pub(crate) mod audit;
pub(crate) mod build_bsp;
pub(crate) mod choose_map;
mod drafts;
mod frame;
mod geometry;
mod keyboard;
mod native;
pub(crate) mod new_map;
pub(crate) mod preferences;
pub(crate) mod project_settings;
pub(crate) mod remap;
mod request;
pub(crate) mod sector;
pub(crate) mod wall;

use crate::{
    app::session::{Damage, EditorSession},
    components::{
        forms::{FieldOwner, FormState, flush_input_registry, heading, labels, row_index},
        types::{PopupGeom, PopupId},
    },
};
use asset_picker::AssetGridState;
use drafts::DialogDrafts;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, KeyDownEvent, PathPromptOptions,
    Pixels, Render, ScrollHandle, ScrollStrategy, SharedString, UniformListScrollHandle, Window,
    div, prelude::*, px, uniform_list,
};
use preferences::PreferencesTab;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Button, ButtonStyle, Color, Label, LabelSize, TintColor, prelude::*};

pub(crate) use request::{DialogRequest, DialogsEvent};

impl EventEmitter<DialogsEvent> for DialogsView {}

pub(crate) struct DialogsView {
    forms: FormState,
    shared: Rc<RefCell<EditorSession>>,
    drafts: DialogDrafts,
    active: Option<PopupId>,
    serial: u64,
    popup_focus: FocusHandle,
    popup_scroll: HashMap<usize, ScrollHandle>,
    preferences_tab: PreferencesTab,
    preferences_scroll: [ScrollHandle; 2],
    asset_grids: HashMap<&'static str, AssetGridState>,
    native_window: Option<gpui::WindowHandle<Self>>,
    native_title: SharedString,
    native_subscriptions: Vec<gpui::Subscription>,
}

impl DialogsView {
    pub(crate) fn new(shared: Rc<RefCell<EditorSession>>, cx: &Context<Self>) -> Self {
        let forms = FormState::new(cx);
        Self {
            popup_focus: forms.focus.clone(),
            forms,
            shared,
            drafts: DialogDrafts::default(),
            active: None,
            serial: 0,
            popup_scroll: HashMap::new(),
            preferences_tab: PreferencesTab::Settings,
            preferences_scroll: Default::default(),
            asset_grids: HashMap::new(),
            native_window: None,
            native_title: SharedString::default(),
            native_subscriptions: Vec::with_capacity(3),
        }
    }

    pub(crate) fn input_registry(
        &self,
    ) -> Rc<RefCell<HashMap<String, Entity<ui_common::TextInput>>>> {
        self.forms.inputs.clone()
    }
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.popup_focus, cx);
    }
    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.forms.text_input_focused(window, cx)
    }
    pub(crate) fn active_popup(&self) -> Option<PopupId> {
        self.active
    }

    pub(crate) fn open(&mut self, request: DialogRequest, window: &Window, cx: &mut Context<Self>) {
        self.close_popup(cx);
        self.serial = self
            .serial
            .checked_add(1)
            .expect("modal generation exhausted");
        self.drafts.pixel_ratio = window.scale_factor();
        self.drafts.document_generation = self.shared.borrow().document_generation;
        let id = {
            let state = &mut *self.shared.borrow_mut();
            match request {
                DialogRequest::Animations => {
                    if !state.ensure_assets() {
                        return;
                    }
                    self.drafts.animations.begin(state.assets.as_ref().unwrap());
                    self.forms.inputs.borrow_mut().clear();
                    PopupId::Animations
                }
                DialogRequest::Preferences => {
                    self.preferences_tab = PreferencesTab::Settings;
                    preferences::actions::begin(&mut self.drafts.prefs, state);
                    PopupId::Prefs
                }
                DialogRequest::ProjectSettings => {
                    project_settings::populate(&mut self.drafts.project_settings, state);
                    PopupId::ProjectSettings
                }
                DialogRequest::BuildBsp => PopupId::BuildBsp,
                DialogRequest::NewMap => PopupId::NewMap,
                DialogRequest::ChooseMap => {
                    self.drafts.maps.maps = state.wad_maps.clone();
                    PopupId::MapList
                }
                DialogRequest::Audit => {
                    audit::actions::refresh(&mut self.drafts, state);
                    PopupId::Audit
                }
                DialogRequest::Remap => {
                    remap::actions::populate(&mut self.drafts, state);
                    PopupId::Remap
                }
                DialogRequest::Wall(index) => {
                    if !wall::actions::open(&mut self.drafts, state, index) {
                        return;
                    }
                    PopupId::WallEdit
                }
                DialogRequest::Sector(index) => {
                    if !sector::actions::open(&mut self.drafts, state, index) {
                        return;
                    }
                    PopupId::SectorEdit
                }
                DialogRequest::AssetPicker {
                    target,
                    current,
                } => {
                    if !asset_picker::actions::populate(&mut self.drafts, state, target, current) {
                        return;
                    }
                    PopupId::Browser
                }
            }
        };
        self.restore_geometry(id);
        self.active = Some(id);
        self.open_native(id, window, cx);
        cx.emit(DialogsEvent::Opened);
        cx.notify();
    }

    pub(crate) fn close_popup(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.active.take() else {
            return;
        };
        self.serial = self
            .serial
            .checked_add(1)
            .expect("modal generation exhausted");
        let damage = {
            let state = &mut *self.shared.borrow_mut();
            match id {
                PopupId::WallEdit => wall::actions::close(&mut self.drafts, state),
                PopupId::SectorEdit => sector::actions::close(&mut self.drafts, state),
                PopupId::Prefs => preferences::actions::close(&self.drafts.prefs, state),
                _ => Damage::None,
            }
        };
        self.persist_geometry(id);
        self.close_native(cx);
        if id == PopupId::Prefs && !matches!(damage, Damage::None) {
            cx.emit(DialogsEvent::PreferencesChanged);
        } else if id != PopupId::Prefs {
            Self::emit_damage(damage, cx);
        }
        cx.emit(DialogsEvent::Closed);
        cx.notify();
    }

    fn emit_damage(damage: Damage, cx: &mut Context<Self>) {
        if !matches!(damage, Damage::None) {
            cx.emit(DialogsEvent::Damage(damage));
        }
        cx.notify();
    }

    fn apply_dialog(&mut self, id: PopupId, cx: &mut Context<Self>) {
        if id == PopupId::Animations {
            match self.drafts.animations.parse() {
                Ok(definitions) => {
                    cx.emit(DialogsEvent::Animations(definitions));
                    self.close_popup(cx);
                }
                Err(error) => {
                    self.drafts.animations.error = error;
                    cx.notify();
                }
            }
            return;
        }
        let damage = {
            let state = &mut *self.shared.borrow_mut();
            match id {
                PopupId::WallEdit => wall::actions::apply(&mut self.drafts, state),
                PopupId::SectorEdit => sector::actions::apply(&mut self.drafts, state),
                PopupId::Prefs => preferences::actions::apply(&mut self.drafts.prefs, state),
                PopupId::ProjectSettings => {
                    project_settings::apply(&self.drafts.project_settings, state);
                    Damage::Restyle
                }
                _ => Damage::None,
            }
        };
        if id == PopupId::Prefs {
            cx.emit(DialogsEvent::PreferencesChanged);
        } else if id == PopupId::ProjectSettings {
            cx.emit(DialogsEvent::ProjectSettingsChanged);
        } else {
            Self::emit_damage(damage, cx);
        }
        self.close_popup(cx);
    }

    fn preview_preferences(&self, cx: &mut Context<Self>) {
        preferences::actions::preview(&self.drafts.prefs, &mut self.shared.borrow_mut());
        cx.emit(DialogsEvent::PreferencesChanged);
        cx.notify();
    }

    pub(crate) fn heal_map(&mut self, cx: &mut Context<Self>) {
        let damage = {
            let state = &mut *self.shared.borrow_mut();
            let damage = state.map_editor.heal_geometry();
            if self.active == Some(PopupId::Audit) {
                audit::actions::refresh(&mut self.drafts, state);
            }
            damage
        };
        Self::emit_damage(damage, cx);
    }

    fn create_map(&mut self, cx: &mut Context<Self>) {
        cx.emit(DialogsEvent::NewMap(self.drafts.new_map.name.clone()));
        self.close_popup(cx);
    }

    fn pick_path(&self, which: PathField, cx: &Context<Self>) {
        let serial = self.serial;
        let resources = self.shared.borrow().resource_generation;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                match which {
                    PathField::Engine => "Choose engine executable",
                    PathField::Iwad | PathField::ProjectIwad => "Choose IWAD",
                }
                .into(),
            ),
        });
        cx.spawn(async move |entity, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.first() else {
                return;
            };
            entity
                .update(cx, |this, cx| {
                    if this.serial != serial
                        || this.active.is_none()
                        || this.shared.borrow().resource_generation != resources
                    {
                        return;
                    }
                    let value = path.display().to_string();
                    match which {
                        PathField::Engine => this.drafts.prefs.engine_path = value,
                        PathField::Iwad => this.drafts.prefs.iwad_path = value,
                        PathField::ProjectIwad => this.drafts.project_settings.iwad_path = value,
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn field_owner(&self, id: PopupId) -> FieldOwner {
        let state = self.shared.borrow();
        match id {
            PopupId::WallEdit => FieldOwner::Wall {
                document: state.document_generation,
                line: self.drafts.wall.line_index,
                side: self.drafts.wall.side_tab,
                slot: self.drafts.wall.selected_slot,
            },
            PopupId::SectorEdit => FieldOwner::Sector {
                document: state.document_generation,
                sector: self.drafts.sector.sector_index,
                slot: self.drafts.sector.selected_slot,
            },
            _ => FieldOwner::Dialog(id),
        }
    }
}

#[derive(Clone, Copy)]
enum PathField {
    Engine,
    Iwad,
    ProjectIwad,
}

impl Render for DialogsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.forms.prepare_frame(window, cx);
        let generation = self.shared.borrow().document_generation;
        if self.drafts.document_generation != generation {
            self.drafts.document_generation = generation;
            self.drafts.wall_edit = None;
            self.drafts.sector_edit = None;
            if self.active.is_some() && self.active != Some(PopupId::Prefs) {
                self.close_popup(cx);
            }
        }
        let Some(id) = self.active else {
            return div().into_any_element();
        };
        self.forms.owner = self.field_owner(id);
        self.update_native_size(window);
        let background = cx.theme().colors().background;
        div()
            .id("editor-dialog-host")
            .debug_selector(|| "editor-dialog".into())
            .track_focus(&self.popup_focus)
            .key_context("Editor")
            .tab_group()
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .text_color(cx.theme().colors().text)
            .text_sm()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ui_common::Quit, window, cx| {
                let inputs = this.input_registry();
                let view = cx.entity();
                window.defer(cx, move |window, cx| {
                    flush_input_registry(&inputs, window, cx);
                    view.update(cx, |_, cx| cx.emit(DialogsEvent::Quit));
                });
                cx.stop_propagation();
            }))
            .when(cfg!(target_os = "macos"), |root| {
                root.child(ui_common::titlebar(self.native_title.clone(), cx).bg(background))
            })
            .child(self.popup(id, window, cx))
            .into_any_element()
    }
}
