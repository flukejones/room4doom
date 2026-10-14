//! Map entity owns native input, presentation drafts and incremental rendering.
pub(crate) mod canvas;
mod commands;
pub(crate) mod inspectors;
mod layout;
pub(crate) mod presentation;
pub(crate) mod preview;
pub(crate) mod render_sync;
mod status;
mod toolbar;
use self::{canvas::CanvasState, inspectors::MapInspector, status::MapStatus, toolbar::MapToolbar};
use crate::app::preferences::ToolbarPositionPref;
use crate::app::session::EditorSession;
use crate::components::forms::{FieldOwner, FormState};
use crate::components::types::SkillFilter;
use crate::dialogs::{DialogRequest, asset_picker::actions::TexBrowseTarget};
use crate::level_editor::state::Damage;
use crate::render::sync as renderer;
pub(crate) use commands::{MapCommand, MapRequest};
use editor_core::Name8;
use gpui::{
    App, Context, Entity, EventEmitter, Focusable as _, Render, Subscription, Window, div,
    prelude::*,
};
pub(crate) use presentation::MapPresentation;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Instant};
use ui_common::ActiveTheme as _;
use ui_common::{NumberInput, NumberInputChanged};
pub(super) enum ControlEvent {
    Command(MapCommand),
    Browse(TexBrowseTarget),
    Dialog(DialogRequest),
}
pub(crate) enum MapEvent {
    Request(MapRequest),
}
impl EventEmitter<MapEvent> for MapEditor {}
pub(crate) struct MapEditor {
    pub(crate) forms: FormState,
    pub(crate) shared: Rc<RefCell<EditorSession>>,
    pub(crate) presentation: MapPresentation,
    pub(crate) canvas: CanvasState,
    pub(crate) modal_active: bool,
    chrome_drag: Option<layout::InspectorDrag>,
    inspector_width: f32,
    polygon_input: Entity<NumberInput>,
    toolbar: Entity<MapToolbar>,
    inspector: Entity<MapInspector>,
    status: Entity<MapStatus>,
    controls_theme: Option<Arc<ui_common::WidgetTheme>>,
    _control_subscriptions: [Subscription; 3],
    _polygon_subscription: Subscription,
}
impl MapEditor {
    pub(crate) fn new(
        shared: Rc<RefCell<EditorSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sides = i32::try_from(shared.borrow().map_editor.ngon_sides).unwrap_or(32);
        let polygon_input =
            cx.new(|cx| NumberInput::new("Polygon sides", sides, 3..=32, window, cx));
        let subscription =
            cx.subscribe(&polygon_input, |this, _, event: &NumberInputChanged, cx| {
                this.command(MapCommand::NgonSidesChosen(event.0), cx);
            });
        let toolbar = cx.new(|cx| MapToolbar {
            forms: FormState::new(cx),
            data: Default::default(),
            vertical: false,
            polygon_input: polygon_input.clone(),
        });
        let inspector = cx.new(|cx| MapInspector::new(shared.clone(), cx));
        let status = cx.new(|cx| MapStatus {
            forms: FormState::new(cx),
            data: Default::default(),
            skill_filter: SkillFilter::All,
        });
        let toolbar_events = cx.subscribe_in(&toolbar, window, |this, _, event, window, cx| {
            this.flush_inspector(window, cx);
            this.control_event(event, cx);
        });
        let status_events = cx.subscribe_in(&status, window, |this, _, event, window, cx| {
            this.flush_inspector(window, cx);
            this.control_event(event, cx);
        });
        let inspector_events =
            cx.subscribe_in(&inspector, window, |this, inspector, event, window, cx| {
                if inspector.read(cx).forms.owner != this.field_owner() {
                    return;
                }
                let committed = this.flush_inspector(window, cx);
                if let ControlEvent::Command(command) = event {
                    if committed == Some(*command) {
                        return;
                    }
                    inspector
                        .read(cx)
                        .copy_draft(*command, &mut this.presentation);
                }
                this.control_event(event, cx);
            });
        Self {
            toolbar,
            inspector,
            status,
            controls_theme: None,
            _control_subscriptions: [toolbar_events, inspector_events, status_events],
            forms: FormState::new(cx),
            shared,
            presentation: MapPresentation::default(),
            canvas: CanvasState::new(),
            modal_active: false,
            chrome_drag: None,
            inspector_width: 300.,
            polygon_input,
            _polygon_subscription: subscription,
        }
    }
    pub(crate) fn input_registry(
        &self,
    ) -> Rc<RefCell<HashMap<String, Entity<ui_common::TextInput>>>> {
        self.forms.inputs.clone()
    }
    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.forms.text_input_focused(window, cx)
            || self.inspector.read(cx).forms.text_input_focused(window, cx)
            || self.polygon_input.focus_handle(cx).is_focused(window)
    }
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_main_content(window, cx);
    }
    pub(crate) fn tab_control(&self, backwards: bool, window: &mut Window, cx: &mut App) {
        let scope = self.canvas_property_popup_focus().cloned();
        let mut first = None;
        loop {
            if backwards {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            let Some(focus) = window.focused(cx) else {
                return;
            };
            if scope
                .as_ref()
                .is_none_or(|scope| scope.contains_focused(window, cx))
            {
                return;
            }
            if first.as_ref() == Some(&focus) {
                return;
            }
            first.get_or_insert(focus);
        }
    }
    fn control_event(&mut self, event: &ControlEvent, cx: &mut Context<Self>) {
        match event {
            ControlEvent::Command(command) => self.command(*command, cx),
            ControlEvent::Browse(target) => {
                Self::request_dialog(self.presentation.asset_request(*target), cx);
            }
            ControlEvent::Dialog(request) => Self::request_dialog(request.clone(), cx),
        }
    }
    fn field_owner(&self) -> FieldOwner {
        let state = self.shared.borrow();
        FieldOwner::Selection {
            document: state.document_generation,
            revision: state.map_editor.selection.revision(),
            sector: state.map_editor.current_sector,
        }
    }
    pub(crate) fn flush_inspector(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<MapCommand> {
        if self.inspector.read(cx).forms.owner != self.field_owner() {
            return None;
        }
        let mut committed = None;
        while let Some(command) = self.inspector.update(cx, |inspector, cx| {
            let command = inspector.accept_pending(window, cx)?;
            inspector.copy_draft(command, &mut self.presentation);
            Some(command)
        }) {
            self.command(command, cx);
            committed = Some(command);
        }
        committed
    }

    fn sync_controls(&mut self, cx: &mut Context<Self>) {
        let theme = cx.theme();
        let theme_changed = self
            .controls_theme
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, theme));
        if theme_changed {
            self.controls_theme = Some(theme.clone());
        }
        let state = self.shared.borrow();
        let vertical = state.prefs.toolbar_position == ToolbarPositionPref::Left;
        let filter = state.map_editor.skill_filter.into();
        let opacity = self.forms.background_opacity;
        let owner = self.forms.owner;
        drop(state);
        self.toolbar.update(cx, |toolbar, cx| {
            if theme_changed
                || toolbar.data != self.presentation.toolbar
                || toolbar.vertical != vertical
            {
                toolbar.data.clone_from(&self.presentation.toolbar);
                toolbar.vertical = vertical;
                cx.notify();
            }
        });
        self.status.update(cx, |status, cx| {
            if theme_changed
                || status.data != self.presentation.status
                || status.skill_filter != filter
            {
                status.data.clone_from(&self.presentation.status);
                status.skill_filter = filter;
                cx.notify();
            }
        });
        self.inspector.update(cx, |inspector, cx| {
            inspector.sync(&self.presentation, owner, cx);
            if theme_changed || inspector.forms.background_opacity != opacity {
                inspector.forms.background_opacity = opacity;
                cx.notify();
            }
        });
    }

    pub(crate) fn command(&mut self, command: MapCommand, cx: &mut Context<Self>) {
        let shared = self.shared.clone();
        let mut state = shared.borrow_mut();
        let effects = self.presentation.command(&mut state, command);
        render_sync::apply_damage(&mut self.presentation, &mut state, effects.damage);
        drop(state);
        if let Some(request) = effects.request {
            cx.emit(MapEvent::Request(request));
        }
        cx.notify();
    }
    pub(crate) fn request_dialog(request: DialogRequest, cx: &mut Context<Self>) {
        cx.emit(MapEvent::Request(MapRequest::Dialog(request)));
    }
    pub(crate) fn refresh(&mut self, damage: Damage, cx: &mut Context<Self>) {
        self.refresh_state(damage);
        cx.notify();
    }
    fn refresh_state(&mut self, damage: Damage) {
        let shared = self.shared.clone();
        let mut state = shared.borrow_mut();
        let document_changed = self.presentation.document != state.document_generation;
        if document_changed {
            let scale = self.presentation.pixel_ratio;
            self.presentation = MapPresentation::default();
            self.presentation.pixel_ratio = scale;
            self.presentation.document = state.document_generation;
            preview::reset(&mut self.presentation, &state);
        }
        if document_changed
            || self.presentation.resources_epoch != state.resource_generation
            || damage == Damage::Restyle
        {
            inspectors::actions::set_special_models(&mut self.presentation, &mut state);
            self.presentation.resources_epoch = state.resource_generation;
            preview::reset(&mut self.presentation, &state);
        }
        self.presentation.toolbar.current = state.map_editor.tool.into();
        self.presentation.toolbar.sector_fill = state.map_editor.sector_fill.into();
        self.presentation.panels_key = None;
        self.presentation.sync(&mut state);
        if state.map_editor.map.is_none() {
            self.presentation.canvas.wgpu_img = Default::default();
        } else {
            render_sync::apply_damage(&mut self.presentation, &mut state, damage);
        }
    }
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        [
            self.presentation.hover_deadline,
            self.presentation.camera_deadline,
            self.presentation.light_deadline,
            self.presentation.bsp_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub(crate) fn tick(&mut self, now: Instant, cx: &mut Context<Self>) {
        if render_sync::tick(&mut self.presentation, &mut self.shared.borrow_mut(), now) {
            cx.notify();
        }
    }
    pub(crate) fn play_build(
        &mut self,
        events: Vec<rbsp::BuildEvent>,
        interval_ms: u64,
        keep_all: bool,
        cx: &mut Context<Self>,
    ) {
        let mut state = self.shared.borrow_mut();
        render_sync::play_build(
            &mut self.presentation,
            &mut state,
            events,
            interval_ms,
            keep_all,
        );
        cx.notify();
    }
    pub(crate) fn apply_asset_pick(
        &mut self,
        target: TexBrowseTarget,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        use crate::boundary::{FlatSlot, TexSlot};
        match target {
            TexBrowseTarget::LineSlot(slot) => {
                let draft = &mut self.presentation.line;
                let (side, touched) = match slot {
                    TexSlot::FrontTop | TexSlot::FrontMid | TexSlot::FrontBottom => {
                        (&mut draft.front, &mut draft.touched)
                    }
                    _ => (&mut draft.back, &mut draft.touched),
                };
                match slot {
                    TexSlot::FrontTop => {
                        side.top = name.into();
                        touched.front_top = true;
                    }
                    TexSlot::FrontMid => {
                        side.mid = name.into();
                        touched.front_mid = true;
                    }
                    TexSlot::FrontBottom => {
                        side.bottom = name.into();
                        touched.front_bottom = true;
                    }
                    TexSlot::BackTop => {
                        side.top = name.into();
                        touched.back_top = true;
                    }
                    TexSlot::BackMid => {
                        side.mid = name.into();
                        touched.back_mid = true;
                    }
                    TexSlot::BackBottom => {
                        side.bottom = name.into();
                        touched.back_bottom = true;
                    }
                }
                self.command(MapCommand::LineApply, cx);
            }
            TexBrowseTarget::SectorSlot(slot) => {
                match slot {
                    FlatSlot::Floor => self.presentation.sector.floor_flat = name.into(),
                    FlatSlot::Ceil => self.presentation.sector.ceil_flat = name.into(),
                }
                self.command(MapCommand::SectorApply, cx);
            }
            TexBrowseTarget::Brush | TexBrowseTarget::DrawFloor | TexBrowseTarget::DrawCeil => {
                if let Ok(name) = Name8::from_dwd_field(name) {
                    let mut state = self.shared.borrow_mut();
                    match target {
                        TexBrowseTarget::Brush => state.map_editor.draw_brush.wall_tex = name,
                        TexBrowseTarget::DrawFloor => state.map_editor.draw_brush.floor_flat = name,
                        TexBrowseTarget::DrawCeil => state.map_editor.draw_brush.ceil_flat = name,
                        _ => {}
                    }
                    self.presentation.panels_key = None;
                    self.presentation.sync(&mut state);
                }
                cx.notify();
            }
            TexBrowseTarget::Patch => {}
        }
    }
}
#[cfg(test)]
mod commands_tests;
impl Render for MapEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.presentation.pixel_ratio = window.scale_factor();
        let refresh = {
            let state = self.shared.borrow();
            self.presentation.document != state.document_generation
                || self.presentation.resources_epoch != state.resource_generation
                || (state.map_editor.map.is_some()
                    && state.wgpu.is_ready()
                    && renderer::assets_changed(&state))
        };
        if refresh {
            self.refresh_state(Damage::Edited);
        }
        self.forms.prepare_frame(window, cx);
        self.forms.owner = {
            let state = self.shared.borrow();
            self.forms.background_opacity =
                if cfg!(target_os = "macos") && state.prefs.window_glass_enabled {
                    state.prefs.window_glass_alpha
                } else {
                    1.
                };
            FieldOwner::Selection {
                document: state.document_generation,
                revision: state.map_editor.selection.revision(),
                sector: state.map_editor.current_sector,
            }
        };
        self.sync_controls(cx);
        let body = self.map_view(window, cx);
        let status = self.status.clone();
        div()
            .debug_selector(|| "map-editor".into())
            .track_focus(&self.forms.focus)
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(div().flex_1().min_h_0().child(body))
            .child(status)
    }
}
