//! Draft properties for newly drawn map geometry.

use super::{DrawField, InspectorField, MapInspector};
use crate::app::session::EditorSession;
use crate::boundary::Tool;
use crate::components::forms::heading;
use crate::dialogs::asset_picker::actions::TexBrowseTarget;
use crate::level_editor::DrawBrush;
use crate::tabs::map_editor::presentation::MapPresentation;
use editor_core::Name8;
use gpui::{AnyElement, Context, Window, div, prelude::*};

#[derive(Default, Clone)]
pub(crate) struct DrawSettingsData {
    pub(crate) active: bool,
    pub(crate) floor_h: String,
    pub(crate) ceil_h: String,
    pub(crate) floor_flat: String,
    pub(crate) ceil_flat: String,
    pub(crate) wall_tex: String,
}

impl MapInspector {
    pub(super) fn draw_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.data.draw_settings;
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading("Draw settings"));
        macro_rules! field {
            ($field:ident,$key:ident,$label:literal) => {
                panel = panel.child(self.fields.field(
                    InspectorField::Draw(DrawField::$key),
                    $label,
                    &c.$field,
                    &self.forms,
                    window,
                    cx,
                ));
            };
        }
        field!(floor_h, Floor, "Floor");
        field!(ceil_h, Ceiling, "Ceiling");
        field!(floor_flat, FloorFlat, "Floor flat");
        panel = panel.child(self.forms.button(
            "draw-floor-browse",
            "Browse floor…",
            cx.listener(|_, _, _, cx| {
                Self::browse(TexBrowseTarget::DrawFloor, cx);
            }),
        ));
        field!(ceil_flat, CeilingFlat, "Ceiling flat");
        panel = panel.child(self.forms.button(
            "draw-ceil-browse",
            "Browse ceiling…",
            cx.listener(|_, _, _, cx| {
                Self::browse(TexBrowseTarget::DrawCeil, cx);
            }),
        ));
        field!(wall_tex, Wall, "Wall");
        panel
            .child(self.forms.button(
                "draw-wall-browse",
                "Browse wall…",
                cx.listener(|_, _, _, cx| {
                    Self::browse(TexBrowseTarget::Brush, cx);
                }),
            ))
            .into_any_element()
    }
}

pub(crate) fn sync(ui: &mut MapPresentation, state: &EditorSession) {
    let draft = &mut ui.draw_settings;
    draft.active = matches!(state.map_editor.tool, Tool::Draw(_));
    if !draft.active {
        return;
    }
    let brush = state.map_editor.draw_brush;
    draft.floor_h = brush.floor_h.to_string();
    draft.ceil_h = brush.ceil_h.to_string();
    draft.floor_flat = brush.floor_flat.to_dwd_field().to_owned();
    draft.ceil_flat = brush.ceil_flat.to_dwd_field().to_owned();
    draft.wall_tex = brush.wall_tex.to_dwd_field().to_owned();
}
pub(crate) fn parse_or(text: &str, fallback: i32) -> i32 {
    text.trim().parse().unwrap_or(fallback)
}
pub(crate) fn name_or(text: &str, fallback: Name8) -> Name8 {
    Name8::from_dwd_field(text.trim()).unwrap_or(fallback)
}
pub(crate) fn apply(ui: &MapPresentation, state: &mut EditorSession) {
    let draft = &ui.draw_settings;
    let old = state.map_editor.draw_brush;
    state.map_editor.draw_brush = DrawBrush {
        floor_h: parse_or(&draft.floor_h, old.floor_h),
        ceil_h: parse_or(&draft.ceil_h, old.ceil_h),
        floor_flat: name_or(&draft.floor_flat, old.floor_flat),
        ceil_flat: name_or(&draft.ceil_flat, old.ceil_flat),
        wall_tex: name_or(&draft.wall_tex, old.wall_tex),
    };
}
