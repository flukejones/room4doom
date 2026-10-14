//! In-window menus for platforms without a native menu bar.
use super::window::EditorShell;
use crate::app::menus as native_menu;
use gpui::{Anchor, App, Div, Window, div, point, prelude::*, px};
use ui_common::widgets::{ContextMenu, DropdownMenu};

impl EditorShell {
    pub(super) fn menu_bar(&self, window: &mut Window, cx: &mut App) -> Div {
        let recent = self.shared.borrow().prefs.recent_projects.clone();
        let busy = self.shared.borrow().job_busy;
        let has_map = self.shared.borrow().map_editor.map.is_some();
        let file = ContextMenu::build(window, cx, move |menu, _, _| {
            let mut menu = menu
                .action("New Project…", Box::new(native_menu::NewProject))
                .action("New Map…", Box::new(native_menu::NewMap))
                .separator()
                .action("Open WAD…", Box::new(native_menu::OpenWad))
                .action("Open Project…", Box::new(native_menu::OpenProject));
            for path in recent {
                menu = menu.action(
                    format!("Recent: {path}"),
                    Box::new(native_menu::OpenRecent(path)),
                );
            }
            menu = menu
                .separator()
                .action("Save Project", Box::new(native_menu::SaveProject))
                .action(
                    "Import DoomEd Project…",
                    Box::new(native_menu::ImportProject),
                )
                .action_checked_with_disabled(
                    "Export WAD…",
                    Box::new(native_menu::ExportWad),
                    false,
                    busy || !has_map,
                );
            for scale in [0.5, 1., 2.] {
                menu = menu.action_checked_with_disabled(
                    format!("Export PNG {scale}×…"),
                    Box::new(native_menu::ExportPng(scale)),
                    false,
                    busy || !has_map,
                );
            }
            menu.separator()
                .action("Close Project", Box::new(native_menu::CloseProject))
                .action("Quit", Box::new(ui_common::Quit))
        });
        let edit = ContextMenu::build(window, cx, |menu, _, _| {
            menu.action("Undo", Box::new(ui_common::Undo))
                .action("Redo", Box::new(ui_common::Redo))
                .separator()
                .action("Copy", Box::new(ui_common::Copy))
                .action("Cut", Box::new(ui_common::Cut))
                .action("Paste", Box::new(ui_common::Paste))
                .separator()
                .action("Project Settings…", Box::new(native_menu::ProjectSettings))
                .action("Preferences…", Box::new(native_menu::Preferences))
                .action("Remap…", Box::new(native_menu::Remap))
        });
        let map = ContextMenu::build(window, cx, |menu, _, _| {
            menu.action("Choose Map…", Box::new(native_menu::ChooseMap))
                .action("Build BSP…", Box::new(native_menu::BuildBsp))
                .action("Heal Geometry", Box::new(native_menu::HealMap))
                .action("Audit Map…", Box::new(native_menu::AuditMap))
        });
        div()
            .flex()
            .gap_1()
            .p_1()
            .child(
                DropdownMenu::new("file-menu", "File", file)
                    .attach(Anchor::BottomLeft)
                    .offset(point(px(0.), px(0.))),
            )
            .child(
                DropdownMenu::new("edit-menu", "Edit", edit)
                    .attach(Anchor::BottomLeft)
                    .offset(point(px(0.), px(0.))),
            )
            .child(
                DropdownMenu::new("map-menu", "Map", map)
                    .attach(Anchor::BottomLeft)
                    .offset(point(px(0.), px(0.))),
            )
    }
}
