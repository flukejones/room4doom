//! Native inspector composition; document diagnostics are read directly.

pub(crate) mod actions;
pub(crate) mod draw;
pub(crate) mod line;
pub(crate) mod sector;
pub(crate) mod thing;
use super::{ControlEvent, MapCommand, MapPresentation};
use crate::app::session::EditorSession;
use crate::components::forms::{FieldOwner, FormState, heading};
use crate::dialogs::asset_picker::actions::TexBrowseTarget;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Focusable as _, Render, Subscription, Window,
    div, prelude::*, px,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use ui_common::ActiveTheme as _;
use ui_common::{InputCommit, TextInput, TextInputEvent};

#[derive(Default)]
pub(super) struct InspectorData {
    pub(super) line: line::LineInspectorData,
    pub(super) thing: thing::ThingInspectorData,
    pub(super) sector: sector::SectorInspectorData,
    pub(super) draw_settings: draw::DrawSettingsData,
}

pub(super) struct MapInspector {
    #[cfg(test)]
    renders: usize,
    pub(super) forms: FormState,
    pub(super) data: InspectorData,
    pub(super) shared: Rc<RefCell<EditorSession>>,
    fields: InspectorFields,
    pub(super) epoch: Option<(u64, u64, u64)>,
}
impl EventEmitter<ControlEvent> for MapInspector {}
impl MapInspector {
    pub(super) fn new(shared: Rc<RefCell<EditorSession>>, cx: &Context<Self>) -> Self {
        Self {
            #[cfg(test)]
            renders: 0,
            forms: FormState::new(cx),
            data: InspectorData::default(),
            shared,
            fields: InspectorFields::default(),
            epoch: None,
        }
    }

    pub(super) fn sync(&mut self, ui: &MapPresentation, owner: FieldOwner, cx: &mut Context<Self>) {
        let epoch = (ui.document, ui.resources_epoch, ui.panels_epoch);
        if self.epoch == Some(epoch) {
            return;
        }
        if self.forms.owner != owner {
            self.fields.0.clear();
            self.forms.inputs.borrow_mut().clear();
            self.forms.owner = owner;
        }
        self.epoch = Some(epoch);
        self.data.line.clone_from(&ui.line);
        self.data.thing.clone_from(&ui.thing);
        self.data.sector.clone_from(&ui.sector);
        self.data.draw_settings.clone_from(&ui.draw_settings);
        cx.notify();
    }

    pub(super) fn copy_draft(&self, command: MapCommand, ui: &mut MapPresentation) {
        match command {
            MapCommand::LineApply => ui.line.clone_from(&self.data.line),
            MapCommand::ThingApply => ui.thing.clone_from(&self.data.thing),
            MapCommand::SectorApply => ui.sector.clone_from(&self.data.sector),
            MapCommand::DrawApply => ui.draw_settings.clone_from(&self.data.draw_settings),
            _ => {}
        }
    }

    fn command(command: MapCommand, cx: &mut Context<Self>) {
        cx.emit(ControlEvent::Command(command));
    }
    fn browse(target: TexBrowseTarget, cx: &mut Context<Self>) {
        cx.emit(ControlEvent::Browse(target));
    }
    fn accept_field(&mut self, key: InspectorField, cx: &App) -> Option<MapCommand> {
        let field = self.fields.0.get_mut(&key)?;
        let value = field.input.read(cx).text();
        if !field.dirty && key.target(&mut self.data).0 == value {
            return None;
        }
        key.write(&mut self.data, value);
        field.dirty = false;
        Some(key.command())
    }
    /// Save/Quit must commit synchronously, before their document command runs.
    pub(super) fn accept_pending(&mut self, window: &Window, cx: &App) -> Option<MapCommand> {
        let key = self
            .fields
            .0
            .iter()
            .find(|(_, field)| field.dirty)
            .or_else(|| {
                self.fields
                    .0
                    .iter()
                    .find(|(_, field)| field.input.focus_handle(cx).is_focused(window))
            })
            .map(|(key, _)| *key)?;
        self.accept_field(key, cx)
    }
}
impl Render for MapInspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.forms.prepare_frame(window, cx);
        self.inspector(window, cx)
    }
}

impl MapInspector {
    pub(super) fn inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut sections = self.forms.panel(cx);
        let inspectors = [
            self.data
                .line
                .active
                .then(|| self.line_inspector(window, cx)),
            self.data
                .thing
                .active
                .then(|| self.thing_inspector(window, cx)),
            self.data
                .sector
                .active
                .then(|| self.sector_inspector(window, cx)),
            self.data
                .draw_settings
                .active
                .then(|| self.draw_settings(window, cx)),
        ];
        let border = cx.theme().colors().border;
        sections = sections.children(inspectors.into_iter().flatten().enumerate().map(
            |(index, inspector)| {
                div()
                    .when(index > 0, |section| {
                        section.border_t_1().border_color(border).pt_2()
                    })
                    .child(inspector)
            },
        ));
        let state = self.shared.borrow();
        if !state.missing_resources.is_empty() {
            sections = sections.child(heading(format!(
                "Missing resources ({})",
                state.missing_resources.len()
            )));
            if let Some(map) = &state.map_editor.map {
                sections = sections.child(format!("Map expects: {}", map.required_wads.join(", ")));
            }
            sections =
                sections.children(state.missing_resources.iter().map(|r| {
                    div().child(format!("{} · {}", r.name.to_dwd_field(), r.kind.label()))
                }));
        }
        div()
            .id("inspector-scroll")
            .debug_selector(|| "map-inspector".into())
            .track_focus(&self.forms.focus)
            .w_full()
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .child(sections)
            .into_any_element()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SideField {
    Top,
    Middle,
    Bottom,
    X,
    Y,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SectorField {
    Floor,
    Ceiling,
    Light,
    Special,
    Tag,
    FloorFlat,
    CeilingFlat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DrawField {
    Floor,
    Ceiling,
    FloorFlat,
    CeilingFlat,
    Wall,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum InspectorField {
    LineSpecial,
    LineTag,
    LineSide { back: bool, part: SideField },
    Sector(SectorField),
    ThingAngle,
    Draw(DrawField),
}
impl InspectorField {
    fn command(self) -> MapCommand {
        match self {
            Self::LineSpecial
            | Self::LineTag
            | Self::LineSide {
                ..
            } => MapCommand::LineApply,
            Self::Sector(_) => MapCommand::SectorApply,
            Self::ThingAngle => MapCommand::ThingApply,
            Self::Draw(_) => MapCommand::DrawApply,
        }
    }
    fn target(self, data: &mut InspectorData) -> (&mut String, Option<&mut bool>) {
        match self {
            Self::LineSpecial => (&mut data.line.special, Some(&mut data.line.touched.special)),
            Self::LineTag => (&mut data.line.tag, Some(&mut data.line.touched.tag)),
            Self::LineSide {
                back,
                part,
            } => {
                let side = if back {
                    &mut data.line.back
                } else {
                    &mut data.line.front
                };
                let touched = &mut data.line.touched;
                let flag = match (back, part) {
                    (false, SideField::Top) => &mut touched.front_top,
                    (false, SideField::Middle) => &mut touched.front_mid,
                    (false, SideField::Bottom) => &mut touched.front_bottom,
                    (false, SideField::X) => &mut touched.front_xoff,
                    (false, SideField::Y) => &mut touched.front_yoff,
                    (true, SideField::Top) => &mut touched.back_top,
                    (true, SideField::Middle) => &mut touched.back_mid,
                    (true, SideField::Bottom) => &mut touched.back_bottom,
                    (true, SideField::X) => &mut touched.back_xoff,
                    (true, SideField::Y) => &mut touched.back_yoff,
                };
                let text = match part {
                    SideField::Top => &mut side.top,
                    SideField::Middle => &mut side.mid,
                    SideField::Bottom => &mut side.bottom,
                    SideField::X => &mut side.xoff,
                    SideField::Y => &mut side.yoff,
                };
                (text, Some(flag))
            }
            Self::Sector(field) => (
                match field {
                    SectorField::Floor => &mut data.sector.floor_h,
                    SectorField::Ceiling => &mut data.sector.ceil_h,
                    SectorField::Light => &mut data.sector.light,
                    SectorField::Special => &mut data.sector.special,
                    SectorField::Tag => &mut data.sector.tag,
                    SectorField::FloorFlat => &mut data.sector.floor_flat,
                    SectorField::CeilingFlat => &mut data.sector.ceil_flat,
                },
                None,
            ),
            Self::ThingAngle => (&mut data.thing.angle, Some(&mut data.thing.touched.angle)),
            Self::Draw(field) => (
                match field {
                    DrawField::Floor => &mut data.draw_settings.floor_h,
                    DrawField::Ceiling => &mut data.draw_settings.ceil_h,
                    DrawField::FloorFlat => &mut data.draw_settings.floor_flat,
                    DrawField::CeilingFlat => &mut data.draw_settings.ceil_flat,
                    DrawField::Wall => &mut data.draw_settings.wall_tex,
                },
                None,
            ),
        }
    }
    fn write(self, data: &mut InspectorData, value: &str) {
        let (target, touched) = self.target(data);
        target.clear();
        target.push_str(value);
        if let Some(touched) = touched {
            *touched = true;
        }
    }
}

#[derive(Default)]
struct InspectorFields(HashMap<InspectorField, InspectorInput>);
struct InspectorInput {
    input: Entity<TextInput>,
    dirty: bool,
    _subscription: Subscription,
}
impl InspectorFields {
    fn field(
        &mut self,
        key: InspectorField,
        label: &'static str,
        value: &str,
        forms: &FormState,
        window: &mut Window,
        cx: &mut Context<MapInspector>,
    ) -> AnyElement {
        let field = self.0.entry(key).or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new(value.to_owned(), window, cx));
            // This registry is only a focus catalog; typed keys own the bindings.
            forms
                .inputs
                .borrow_mut()
                .insert(format!("{key:?}"), input.clone());
            let subscription =
                cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
                    let Some(field) = this
                        .fields
                        .0
                        .get_mut(&key)
                        .filter(|field| field.input == *input)
                    else {
                        return;
                    };
                    match event {
                        TextInputEvent::Changed(value) => {
                            key.write(&mut this.data, value);
                            field.dirty = true;
                        }
                        TextInputEvent::Committed(reason) => {
                            if let Some(command) = this.accept_field(key, cx) {
                                MapInspector::command(command, cx);
                            }
                            if *reason == InputCommit::Submit {
                                window.focus(&this.forms.focus, cx);
                            }
                        }
                    }
                });
            InspectorInput {
                input,
                dirty: false,
                _subscription: subscription,
            }
        });
        field
            .input
            .update(cx, |input, cx| input.set_text(value, window, cx));
        div()
            .flex()
            .items_center()
            .gap_2()
            .when(!label.is_empty(), |row| {
                row.child(div().w(px(110.)).flex_shrink_0().child(label))
            })
            .child(div().flex_1().min_w_0().child(field.input.clone()))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::preferences::ToolbarPositionPref;
    use crate::level_editor::{LevelEditorState, state::Damage};
    use crate::render::input::SelItem;
    use crate::tabs::map_editor::{MapEditor, canvas};
    use gpui::{TestAppContext, VisualTestContext, WindowHandle, size};

    fn open_map(cx: &mut TestAppContext) -> WindowHandle<MapEditor> {
        cx.update(|cx| {
            ui_common::init("Map editor test", cx);
            canvas::bind_keys(cx);
        });
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").unwrap();
        let first = map.lines.keys().next().unwrap();
        let mut state = LevelEditorState::new();
        state.load_map(map, "E1M1");
        state.selection.replace(SelItem::Line(first));
        let shared = Rc::new(RefCell::new(EditorSession::new(
            state,
            None,
            Default::default(),
        )));
        let root = cx.open_window(size(px(1100.), px(800.)), |window, cx| {
            MapEditor::new(shared.clone(), window, cx)
        });
        cx.update_window(root.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        // Settle the initial viewport/fit notification before measuring camera redraws.
        root.update(cx, |editor, _, cx| {
            editor.command(MapCommand::Pan(0., 0.), cx);
        })
        .unwrap();
        root
    }

    #[gpui::test]
    fn camera_redraws_preserve_control_layout_and_reuse_inspector(cx: &mut TestAppContext) {
        let root = open_map(cx);
        let mut visual = VisualTestContext::from_window(root.into(), cx);
        let check_layout = |visual: &mut VisualTestContext, vertical| {
            let frame = visual.debug_bounds("map-editor").unwrap();
            let toolbar = visual.debug_bounds("map-toolbar").unwrap();
            let status = visual.debug_bounds("map-status").unwrap();
            let inspector = visual.debug_bounds("map-inspector").unwrap();
            let canvas = visual.debug_bounds("map-canvas").unwrap();
            assert!(
                toolbar.size.height >= px(24.),
                "toolbar bounds: {toolbar:?}"
            );
            assert!(status.size.height >= px(24.), "status bounds: {status:?}");
            if vertical {
                assert!(
                    toolbar.right() <= canvas.left(),
                    "toolbar: {toolbar:?}, canvas: {canvas:?}"
                );
            } else {
                assert!(
                    toolbar.bottom() <= canvas.top(),
                    "toolbar: {toolbar:?}, canvas: {canvas:?}"
                );
            }
            assert!(canvas.right() <= inspector.left());
            assert!(
                inspector.bottom() <= status.top(),
                "inspector: {inspector:?}, status: {status:?}"
            );
            assert!(
                status.bottom() <= frame.bottom(),
                "status: {status:?}, frame: {frame:?}"
            );
        };
        check_layout(&mut visual, false);
        let before = root
            .update(cx, |editor, _, cx| editor.inspector.read(cx).renders)
            .unwrap();
        for n in 0_u16..8 {
            root.update(cx, |editor, _, cx| {
                editor.command(MapCommand::Hover(100. + f32::from(n), 200.), cx);
                editor.command(MapCommand::Pan(1., 0.), cx);
            })
            .unwrap();
        }
        root.update(cx, |editor, _, cx| {
            assert_eq!(
                editor.inspector.read(cx).renders,
                before,
                "hover and pan must reuse the cached inspector"
            );
        })
        .unwrap();
        for width in [720., 1720.] {
            visual.simulate_resize(size(px(width), px(600.)));
            visual.update(|window, cx| window.draw(cx).clear(cx));
            check_layout(&mut visual, false);
        }
        root.update(&mut visual, |editor, _, cx| {
            editor.shared.borrow_mut().prefs.toolbar_position = ToolbarPositionPref::Left;
            editor.refresh(Damage::Restyle, cx);
        })
        .unwrap();
        visual.simulate_resize(size(px(720.), px(600.)));
        visual.update(|window, cx| window.draw(cx).clear(cx));
        check_layout(&mut visual, true);
    }

    #[gpui::test]
    fn inspector_edits_flush_and_stay_with_their_selection(cx: &mut TestAppContext) {
        let root = open_map(cx);
        let shared = root
            .update(cx, |editor, _, _| editor.shared.clone())
            .unwrap();
        let (first, second) = {
            let state = shared.borrow();
            let mut keys = state.map_editor.map.as_ref().unwrap().lines.keys();
            (keys.next().unwrap(), keys.next().unwrap())
        };
        let key = InspectorField::LineSide {
            back: false,
            part: SideField::X,
        };
        let input = root
            .update(cx, |editor, window, cx| {
                let input = editor.inspector.read(cx).fields.0[&key].input.clone();
                window.focus(&input.focus_handle(cx), cx);
                input
            })
            .unwrap();
        cx.dispatch_action(root.into(), ui_common::SelectAll);
        cx.simulate_input(root.into(), "37");
        // Save/Quit use this same synchronous flush, without waiting for blur.
        root.update(cx, |editor, window, cx| {
            editor.flush_inspector(window, cx);
            let state = shared.borrow();
            let map = state.map_editor.map.as_ref().unwrap();
            assert_eq!(map.lines[first].front.x_offset, 37);
        })
        .unwrap();
        cx.dispatch_action(root.into(), ui_common::SelectAll);
        cx.simulate_input(root.into(), "41");
        cx.simulate_keystrokes(root.into(), "enter");
        assert_eq!(
            shared.borrow().map_editor.map.as_ref().unwrap().lines[first]
                .front
                .x_offset,
            41
        );
        root.update(cx, |editor, window, cx| {
            editor.command(MapCommand::Undo, cx);
            window.focus(&input.focus_handle(cx), cx);
        })
        .unwrap();
        assert_eq!(
            shared.borrow().map_editor.map.as_ref().unwrap().lines[first]
                .front
                .x_offset,
            37
        );
        let grid = shared.borrow().map_editor.grid;
        cx.simulate_keystrokes(root.into(), "g");
        assert_eq!(
            shared.borrow().map_editor.grid,
            grid,
            "a focused field consumes canvas shortcuts"
        );
        root.update(cx, |editor, window, cx| {
            editor.flush_inspector(window, cx);
            editor.focus(window, cx);
        })
        .unwrap();
        cx.simulate_keystrokes(root.into(), "g");
        assert_ne!(
            shared.borrow().map_editor.grid,
            grid,
            "canvas focus enables its bindings"
        );
        root.update(cx, |editor, _, cx| {
            shared
                .borrow_mut()
                .map_editor
                .selection
                .replace(SelItem::Line(second));
            editor.refresh(Damage::None, cx);
        })
        .unwrap();
        cx.update_window(root.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let replacement = root
            .update(cx, |editor, window, cx| {
                let input = editor.inspector.read(cx).fields.0[&key].input.clone();
                window.focus(&input.focus_handle(cx), cx);
                input
            })
            .unwrap();
        assert_ne!(
            input, replacement,
            "field history belongs to the selected object"
        );
        let original = root
            .update(cx, |_, _, cx| replacement.read(cx).text().to_owned())
            .unwrap();
        cx.dispatch_action(root.into(), ui_common::Undo);
        root.update(cx, |_, _, cx| {
            assert_eq!(replacement.read(cx).text(), original);
        })
        .unwrap();
    }
}
