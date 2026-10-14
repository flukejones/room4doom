//! Native zoom, grid, snapping and display controls.

use super::presentation::MapPresentation;
use super::{ControlEvent, MapCommand};
use crate::app::session::EditorSession;
use crate::components::forms::FormState;
use crate::components::{forms::labels, types::SkillFilter};
use crate::dialogs::asset_picker::actions::TexBrowseTarget;
use gpui::{AnyElement, Context, Div, EventEmitter, Render, Window, div, prelude::*};
use std::fmt::Write as _;
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Color, Label, Toggleable as _};

#[derive(Clone, PartialEq)]
pub(crate) struct StatusData {
    pub(crate) zoom: String,
    pub(crate) grid: String,
    pub(crate) snap: bool,
    pub(crate) snap_vertex: bool,
    pub(crate) snap_line: bool,
    pub(crate) angle_snap: bool,
    pub(crate) highlight_unenclosed: bool,
    pub(crate) overlays_visible: bool,
    pub(crate) brush: String,
}

impl Default for StatusData {
    fn default() -> Self {
        Self {
            zoom: String::new(),
            grid: String::new(),
            snap: true,
            snap_vertex: false,
            snap_line: false,
            angle_snap: false,
            highlight_unenclosed: false,
            overlays_visible: true,
            brush: "brush: -".into(),
        }
    }
}

pub(super) struct MapStatus {
    pub(super) forms: FormState,
    pub(super) data: StatusData,
    pub(super) skill_filter: SkillFilter,
}
impl EventEmitter<ControlEvent> for MapStatus {}
impl Render for MapStatus {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.forms.prepare_frame(window, cx);
        self.status_bar(window, cx)
    }
}
impl MapStatus {
    fn command(command: MapCommand, cx: &mut Context<Self>) {
        cx.emit(ControlEvent::Command(command));
    }

    pub(super) fn status_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.data;
        let mut bar = div()
            .debug_selector(|| "map-status".into())
            .track_focus(&self.forms.focus)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().colors().border);
        bar = bar
            .child(self.view_controls(window, cx))
            .child(self.snap_controls(cx));
        for (id, label, value, command) in [
            (
                "void",
                "Void",
                c.highlight_unenclosed,
                MapCommand::HighlightUnenclosedToggled,
            ),
            (
                "overlays",
                "Overlays",
                c.overlays_visible,
                MapCommand::ToggleOverlays,
            ),
        ] {
            bar = bar.child(
                self.forms
                    .button(
                        id,
                        label,
                        cx.listener(move |_, _, _, cx| Self::command(command, cx)),
                    )
                    .toggle_state(value),
            );
        }
        let selected = match self.skill_filter {
            SkillFilter::All => 0,
            SkillFilter::Easy => 1,
            SkillFilter::Normal => 2,
            SkillFilter::Hard => 3,
        };
        let filter = cx.listener(|_, index: &i32, _, cx| {
            if let Some(filter) = usize::try_from(*index).ok().and_then(|i| {
                [
                    SkillFilter::All,
                    SkillFilter::Easy,
                    SkillFilter::Normal,
                    SkillFilter::Hard,
                ]
                .get(i)
                .copied()
            }) {
                Self::command(MapCommand::ThingsSelected(filter), cx);
            }
        });
        bar.child(self.forms.choices(
            "thing-filter",
            labels(&["All skills", "Easy", "Normal", "Hard"]),
            selected,
            filter,
            window,
            cx,
        ))
        .child(self.forms.button(
            "brush",
            c.brush.clone(),
            cx.listener(|_, _, _, cx| {
                cx.emit(ControlEvent::Browse(TexBrowseTarget::Brush));
            }),
        ))
        .into_any_element()
    }
    fn snap_controls(&self, cx: &Context<Self>) -> Div {
        let c = &self.data;
        let mut group = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_sm()
            .child(Label::new("Snap").color(Color::Muted));
        for (id, label, value, command) in [
            ("snap", "Grid", c.snap, MapCommand::SnapToggled),
            (
                "snap-vertex",
                "Vertex",
                c.snap_vertex,
                MapCommand::SnapVertexToggled,
            ),
            (
                "snap-line",
                "Line",
                c.snap_line,
                MapCommand::SnapLineToggled,
            ),
            (
                "snap-angle",
                "Angle",
                c.angle_snap,
                MapCommand::AngleSnapToggled,
            ),
        ] {
            group = group.child(
                self.forms
                    .button(
                        id,
                        label,
                        cx.listener(move |_, _, _, cx| Self::command(command, cx)),
                    )
                    .toggle_state(value),
            );
        }
        group
    }
    fn view_controls(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = &self.data;
        let zoom = cx.listener(|_, index: &i32, _, cx| {
            if let Some(level) = usize::try_from(*index)
                .ok()
                .and_then(|i| [0., 0.25, 0.5, 1., 2., 4., 8.].get(i).copied())
            {
                Self::command(MapCommand::ZoomSelected(level), cx);
            }
        });
        let bar = div().flex().gap_2().child(self.forms.choice_menu(
            "zoom-level",
            c.zoom.clone(),
            labels(&["Fit map", "25%", "50%", "100%", "200%", "400%", "800%"]),
            zoom,
            window,
            cx,
        ));
        let grid = cx.listener(|_, index: &i32, _, cx| {
            if let Some(grid) = usize::try_from(*index)
                .ok()
                .and_then(|i| [1, 2, 4, 8, 16, 32, 64].get(i).copied())
            {
                Self::command(MapCommand::GridSelected(grid), cx);
            }
        });
        bar.child(self.forms.choice_menu(
            "grid-size",
            c.grid.clone(),
            labels(&["1", "2", "4", "8", "16", "32", "64"]),
            grid,
            window,
            cx,
        ))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "display coordinates round floating point Doom units"
)]
pub(crate) fn update_status(ui: &mut MapPresentation, state: &EditorSession) {
    let app = &state.map_editor;
    let [x, y] = app.cursor_world;
    ui.canvas.coords.clear();
    ui.status.zoom.clear();
    ui.status.grid.clear();
    let _ = write!(
        ui.canvas.coords,
        "({}, {})",
        x.round() as i32,
        y.round() as i32
    );
    let _ = write!(ui.status.zoom, "zoom {:.2}x", app.camera.zoom_level());
    let _ = write!(ui.status.grid, "grid {}", app.grid);
    ui.status.snap = app.snap;
    ui.status.snap_vertex = app.snap_to_vertex;
    ui.status.snap_line = app.snap_to_line;
    ui.status.angle_snap = app.angle_snap;
    ui.status.highlight_unenclosed = app.highlight_unenclosed;
    ui.status.overlays_visible = app.overlays_visible;
    let brush = app.draw_brush.wall_tex;
    ui.status.brush.clear();
    let _ = write!(ui.status.brush, "brush: {}", brush.to_dwd_field());
}
