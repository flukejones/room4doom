//! Native dialog contents with shared scrolling and footer layout.

use super::*;
use ui_common::widgets::{Modal, ModalFooter, Section};

impl DialogsView {
    pub(super) fn window_title(&self, id: PopupId) -> SharedString {
        match id {
            PopupId::MapList => "Choose map".into(),
            PopupId::Browser => self.drafts.asset_picker.title.clone().into(),
            PopupId::Remap => "Remap".into(),
            PopupId::Prefs => "Preferences".into(),
            PopupId::WallEdit => {
                format!("Wall editor · Line {}", self.drafts.wall.line_index).into()
            }
            PopupId::SectorEdit => {
                format!("Sector editor · Sector {}", self.drafts.sector.sector_index).into()
            }
            PopupId::ProjectSettings => "Project settings".into(),
            PopupId::BuildBsp => "Build BSP".into(),
            PopupId::NewMap => "New map".into(),
            PopupId::Audit => "Map audit".into(),
            PopupId::Animations => "Animations".into(),
        }
    }

    pub(super) fn default_size(id: PopupId) -> gpui::Size<Pixels> {
        let width = match id {
            PopupId::MapList => 460.,
            PopupId::Browser | PopupId::SectorEdit => 840.,
            PopupId::Remap | PopupId::Animations => 520.,
            PopupId::Prefs => 640.,
            PopupId::WallEdit => 960.,
            PopupId::ProjectSettings => 600.,
            PopupId::BuildBsp => 480.,
            PopupId::NewMap => 420.,
            PopupId::Audit => 680.,
        };
        let height = match id {
            PopupId::NewMap | PopupId::BuildBsp => 280.,
            PopupId::Prefs | PopupId::WallEdit | PopupId::SectorEdit => 640.,
            _ => 520.,
        };
        gpui::size(px(width), px(height))
    }

    pub(super) fn popup(
        &mut self,
        id: PopupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match id {
            PopupId::MapList => self.map_list(cx),
            PopupId::Browser => self.asset_browser(window, cx),
            PopupId::Remap => self.remap(window, cx),
            PopupId::Prefs => self.preferences(window, cx),
            PopupId::WallEdit => self.wall_editor(window, cx),
            PopupId::SectorEdit => self.sector_editor(window, cx),
            PopupId::ProjectSettings => self.project_settings(window, cx),
            PopupId::BuildBsp => self.build_bsp(window, cx),
            PopupId::NewMap => self.new_map(window, cx),
            PopupId::Audit => self.audit(cx),
            PopupId::Animations => self.animations(window, cx),
        };
        let fill = matches!(
            id,
            PopupId::Browser | PopupId::WallEdit | PopupId::SectorEdit | PopupId::Prefs
        );
        let content =
            if fill || matches!(id, PopupId::Prefs | PopupId::MapList | PopupId::Animations) {
                div()
                    .flex()
                    .flex_col()
                    .when(fill, |content| content.flex_1().min_h_0())
                    .when(!fill, |content| content.flex_none())
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .child(content)
                    .into_any_element()
            } else {
                Section::new().child(content).into_any_element()
            };
        let footer = match id {
            PopupId::Prefs | PopupId::WallEdit | PopupId::SectorEdit | PopupId::Animations => {
                Some(self.dialog_buttons(id, cx))
            }
            PopupId::ProjectSettings if self.drafts.project_settings.active => {
                Some(self.dialog_buttons(id, cx))
            }
            _ => None,
        };
        let scroll = self.popup_scroll.entry(id as usize).or_default().clone();
        // Modal's scroll container constrains the tabbed body's height.
        Modal::new(
            format!("editor-dialog-{id:?}"),
            (!fill || id == PopupId::Prefs).then_some(scroll),
        )
        .child(content)
        .when_some(footer, |modal, footer| modal.footer(footer))
        .into_any_element()
    }

    fn dialog_buttons(&self, id: PopupId, cx: &Context<Self>) -> ModalFooter {
        ModalFooter::new().end_slot(
            div()
                .flex()
                .gap_2()
                .justify_end()
                .child(self.forms.button(
                    "dialog-cancel",
                    "Cancel",
                    cx.listener(|this, _, _, cx| this.close_popup(cx)),
                ))
                .child(self.forms.button(
                    "dialog-apply",
                    "Apply",
                    cx.listener(move |this, _, _, cx| this.apply_dialog(id, cx)),
                )),
        )
    }
}
