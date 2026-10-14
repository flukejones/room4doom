//! Native tool selectors and drawing/render controls.

use super::{ControlEvent, MapCommand};
use crate::components::forms::FormState;
use crate::components::{
    forms::{labels, row_index},
    types::{SectorFill, ToolKind},
};
use crate::dialogs::DialogRequest;
use gpui::{
    AnyElement, Context, Div, Entity, EventEmitter, Render, SharedString, Window, div, prelude::*,
    px,
};
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Color, Icon, IconName, Label, prelude::*};

#[derive(Clone, PartialEq)]
pub(crate) struct ToolbarData {
    pub(crate) current: ToolKind,
    pub(crate) last_select: ToolKind,
    pub(crate) last_shape: ToolKind,
    pub(crate) sector_fill: SectorFill,
    pub(crate) projection_perspective: bool,
}

impl Default for ToolbarData {
    fn default() -> Self {
        Self {
            current: ToolKind::Select,
            last_select: ToolKind::Select,
            last_shape: ToolKind::Line,
            sector_fill: SectorFill::None,
            projection_perspective: Default::default(),
        }
    }
}

const SELECT_TOOLS: [(&str, ToolKind, IconName); 5] = [
    ("Select", ToolKind::Select, IconName::EditorCursor),
    ("Select Vertex", ToolKind::SelectVertex, IconName::Crosshair),
    ("Select Line", ToolKind::SelectLine, IconName::Dash),
    ("Select Thing", ToolKind::SelectThing, IconName::Person),
    ("Select Sector", ToolKind::SelectSector, IconName::Box),
];
const DRAW_TOOLS: [(&str, ToolKind, IconName); 4] = [
    ("Line", ToolKind::Line, IconName::Dash),
    ("Rectangle", ToolKind::Rect, IconName::SquareDot),
    ("Triangle", ToolKind::Triangle, IconName::Triangle),
    ("Polygon", ToolKind::Ngon, IconName::SelectAll),
];
pub(super) struct MapToolbar {
    pub(super) forms: FormState,
    pub(super) data: ToolbarData,
    pub(super) vertical: bool,
    pub(super) polygon_input: Entity<ui_common::NumberInput>,
}
impl EventEmitter<ControlEvent> for MapToolbar {}
impl Render for MapToolbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.forms.prepare_frame(window, cx);
        self.tool_strip(window, cx)
    }
}
impl MapToolbar {
    fn command(command: MapCommand, cx: &mut Context<Self>) {
        cx.emit(ControlEvent::Command(command));
    }

    pub(super) fn tool_strip(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let polygon = (self.data.current == ToolKind::Ngon).then(|| self.polygon_input.clone());
        let tools = &self.data;
        let vertical = self.vertical;
        let mut strip = div()
            .debug_selector(|| "map-toolbar".into())
            .track_focus(&self.forms.focus)
            .flex()
            .gap_1()
            .p_1()
            .flex_shrink_0()
            .when(vertical, |d| d.flex_col().w(px(176.)))
            .when(!vertical, |d| d.w_full().flex_wrap().items_center());
        let [select, draw] = self.tool_selectors(vertical, window, cx);
        strip = strip.child(select);
        let mut edit = div()
            .flex()
            .gap_1()
            .p_1()
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_sm()
            .when(vertical, |d| d.flex_col().mb_4())
            .when(!vertical, |d| d.flex_wrap().items_center())
            .child(Label::new("Edit").color(Color::Muted))
            .child(draw);
        if let Some(polygon) = polygon {
            edit = edit.child(polygon);
        }
        for (label, tool, icon) in [
            ("Thing", ToolKind::Thing, IconName::Plus),
            ("Sector", ToolKind::Sector, IconName::Box),
        ] {
            edit = edit.child(
                self.forms
                    .button(
                        label,
                        label,
                        cx.listener(move |_, _, _, cx| {
                            Self::command(MapCommand::ToolChanged(tool), cx);
                        }),
                    )
                    .start_icon(
                        Icon::new(icon).color(self.forms.button_foreground(tools.current == tool)),
                    )
                    .toggle_state(tools.current == tool)
                    .when(vertical, |button| button.full_width()),
            );
        }
        strip = strip
            .child(edit)
            .when(!vertical, |d| d.child(div().flex_1().min_w(px(16.))))
            .child(
                self.forms
                    .button(
                        "Test Map",
                        "Test Map",
                        cx.listener(|_, _, _, cx| {
                            Self::command(MapCommand::ToolChanged(ToolKind::Launch), cx);
                        }),
                    )
                    .start_icon(
                        Icon::new(IconName::PlayFilled).color(
                            self.forms
                                .button_foreground(tools.current == ToolKind::Launch),
                        ),
                    )
                    .toggle_state(tools.current == ToolKind::Launch)
                    .when(vertical, |button| button.full_width()),
            )
            .child(self.render_controls(vertical, window, cx));
        if vertical {
            div()
                .id("tools-scroll")
                .h_full()
                .overflow_y_scroll()
                .child(strip)
                .into_any_element()
        } else {
            strip.into_any_element()
        }
    }
    fn tool_selectors(
        &self,
        vertical: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 2] {
        let tools = &self.data;
        let select_labels = SELECT_TOOLS.map(|(label, _, _)| SharedString::from(label));
        let draw_labels = DRAW_TOOLS.map(|(label, _, _)| SharedString::from(label));
        [
            ("select-tool", &SELECT_TOOLS[..], tools.last_select),
            ("draw-tool", &DRAW_TOOLS[..], tools.last_shape),
        ]
        .map(|(id, options, last)| {
            let selected = options
                .iter()
                .position(|(_, kind, _)| *kind == last)
                .unwrap_or(0);
            let (label, _, icon) = options[selected];
            let active = options.iter().any(|(_, kind, _)| *kind == tools.current);
            let primary = self
                .forms
                .button(
                    id,
                    label,
                    cx.listener(move |_, _, _, cx| {
                        Self::command(MapCommand::ToolChanged(last), cx);
                    }),
                )
                .start_icon(Icon::new(icon).color(self.forms.button_foreground(active)))
                .toggle_state(active)
                .when(vertical, |b| b.full_width());
            let names: &[SharedString] = if id == "select-tool" {
                &select_labels
            } else {
                &draw_labels
            };
            let listener = cx.listener(move |_, index: &i32, _, cx| {
                if let Some((_, kind, _)) = usize::try_from(*index)
                    .ok()
                    .and_then(|index| options.get(index))
                {
                    Self::command(MapCommand::ToolChanged(*kind), cx);
                }
            });
            self.forms
                .split_choices(
                    id,
                    primary,
                    names,
                    row_index(selected),
                    listener,
                    window,
                    cx,
                )
                .into_any_element()
        })
    }
    fn render_controls(&self, vertical: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let tools = &self.data;
        let fill = match tools.sector_fill {
            SectorFill::None => 0,
            SectorFill::Colour => 1,
            SectorFill::Texture => 2,
        };
        let listener = cx.listener(|_, index: &i32, _, cx| {
            if let Some(mode) = usize::try_from(*index).ok().and_then(|i| {
                [SectorFill::None, SectorFill::Colour, SectorFill::Texture]
                    .get(i)
                    .copied()
            }) {
                Self::command(MapCommand::FillChanged(mode), cx);
            }
        });
        div()
            .flex()
            .gap_1()
            .when(vertical, |d| d.flex_col())
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .when(vertical, |d| d.flex_col())
                    .child(Label::new("Render type"))
                    .child(
                        self.forms
                            .choices(
                                "sector-fill",
                                labels(&["None", "Colour", "Texture"]),
                                fill,
                                listener,
                                window,
                                cx,
                            )
                            .full_width(vertical),
                    ),
            )
            .child(
                self.forms
                    .button(
                        "perspective",
                        "Perspective",
                        cx.listener(|this, _, _, cx| {
                            let perspective = !this.data.projection_perspective;
                            Self::command(MapCommand::ProjectionSelected(perspective), cx);
                        }),
                    )
                    .toggle_state(tools.projection_perspective)
                    .when(vertical, |button| button.full_width()),
            )
            .child(self.forms.button(
                "reset-view",
                "Reset View",
                cx.listener(|_, _, _, cx| Self::command(MapCommand::ResetView, cx)),
            ))
            .child(self.forms.button(
                "test-bsp",
                "Test BSP",
                cx.listener(|_, _, _, cx| cx.emit(ControlEvent::Dialog(DialogRequest::BuildBsp))),
            ))
    }
}
