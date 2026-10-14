//! Sector height, lighting, special and flat drafts.

use super::{InspectorField, MapInspector, SectorField};
use crate::boundary::FlatSlot;
use crate::components::forms::heading;
use crate::dialogs::asset_picker::actions::TexBrowseTarget;
use crate::tabs::map_editor::MapCommand;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct SectorInspectorData {
    pub(crate) active: bool,
    pub(crate) sector_index: i32,
    pub(crate) floor_h: String,
    pub(crate) ceil_h: String,
    pub(crate) light: String,
    pub(crate) special: String,
    pub(crate) specials: Rc<[(i32, SharedString)]>,
    pub(crate) tag: String,
    pub(crate) floor_flat: String,
    pub(crate) ceil_flat: String,
}

impl Default for SectorInspectorData {
    fn default() -> Self {
        Self {
            active: false,
            sector_index: -1,
            floor_h: Default::default(),
            ceil_h: Default::default(),
            light: Default::default(),
            special: Default::default(),
            specials: Default::default(),
            tag: Default::default(),
            floor_flat: Default::default(),
            ceil_flat: Default::default(),
        }
    }
}

impl MapInspector {
    pub(super) fn sector_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.data.sector;
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading(format!("Sector {}", c.sector_index)));
        macro_rules! text {
            ($field:ident,$key:ident,$label:literal) => {
                panel = panel.child(self.fields.field(
                    InspectorField::Sector(SectorField::$key),
                    $label,
                    &c.$field,
                    &self.forms,
                    window,
                    cx,
                ));
            };
        }
        text!(floor_h, Floor, "Floor");
        text!(ceil_h, Ceiling, "Ceiling");
        text!(light, Light, "Light");
        text!(special, Special, "Special");
        text!(tag, Tag, "Tag");
        panel = panel.child(self.forms.typed_select_field(
            "sector-special-choice",
            "Special preset",
            c.specials.clone(),
            c.special.trim().parse::<i32>().ok().as_ref(),
            cx.listener(|this, value: &i32, _, cx| {
                this.data.sector.special = value.to_string();
                Self::command(MapCommand::SectorApply, cx);
            }),
            window,
            cx,
        ));
        for (label, slot, value) in [
            ("Floor flat", FlatSlot::Floor, &c.floor_flat),
            ("Ceiling flat", FlatSlot::Ceil, &c.ceil_flat),
        ] {
            let key = if slot == FlatSlot::Floor {
                SectorField::FloorFlat
            } else {
                SectorField::CeilingFlat
            };
            let field = self.fields.field(
                InspectorField::Sector(key),
                label,
                value,
                &self.forms,
                window,
                cx,
            );
            panel = panel.child(
                div()
                    .flex()
                    .gap_1()
                    .child(div().flex_1().child(field))
                    .child(self.forms.button(
                        format!("browse-flat-{slot:?}"),
                        "…",
                        cx.listener(move |_, _, _, cx| {
                            Self::browse(TexBrowseTarget::SectorSlot(slot), cx);
                        }),
                    )),
            );
        }
        panel
            .child(self.forms.button(
                "new-sector",
                "New sector",
                cx.listener(|_, _, _, cx| Self::command(MapCommand::SectorNewSector, cx)),
            ))
            .into_any_element()
    }
}
