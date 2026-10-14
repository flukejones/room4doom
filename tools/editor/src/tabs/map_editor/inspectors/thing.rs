//! Thing placement and selection drafts.

use super::{InspectorField, MapInspector};
use crate::components::forms::heading;
use crate::components::types::{ThingOptions, ThingTouched};
use crate::tabs::map_editor::MapCommand;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use std::rc::Rc;

#[derive(Default, Clone)]
pub(crate) struct ThingInspectorData {
    pub(crate) active: bool,
    pub(crate) types: Rc<[(i32, SharedString)]>,
    pub(crate) kind: Option<i32>,
    pub(crate) angle: String,
    pub(crate) options: ThingOptions,
    pub(crate) touched: ThingTouched,
}

impl MapInspector {
    pub(super) fn thing_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.data.thing;
        let mut panel = div().flex().flex_col().gap_2().child(heading("Thing"));
        panel = panel
            .child(self.forms.typed_select_field(
                "thing-type",
                "Type",
                c.types.clone(),
                c.kind.as_ref(),
                cx.listener(|this, kind: &i32, _, cx| {
                    this.data.thing.kind = Some(*kind);
                    this.data.thing.touched.r#type = true;
                    Self::command(MapCommand::ThingApply, cx);
                }),
                window,
                cx,
            ))
            .child(self.fields.field(
                InspectorField::ThingAngle,
                "Angle",
                &c.angle,
                &self.forms,
                window,
                cx,
            ));
        macro_rules! option {
            ($name:ident,$label:literal) => {
                panel = panel.child(self.forms.check(
                    concat!("thing-", stringify!($name)),
                    $label,
                    c.options.$name,
                    cx.listener(|this, value: &bool, _, cx| {
                        this.data.thing.options.$name = *value;
                        this.data.thing.touched.$name = true;
                        Self::command(MapCommand::ThingApply, cx);
                    }),
                ));
            };
        }
        option!(easy, "Easy");
        option!(normal, "Normal");
        option!(hard, "Hard");
        option!(ambush, "Ambush");
        option!(multi, "Multiplayer only");
        panel.into_any_element()
    }
}
