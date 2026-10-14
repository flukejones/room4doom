//! Native application menus; each action delegates to the existing editor callbacks.

use crate::tabs::{map_editor::canvas, texture_editor};
use gpui::{App, KeyBinding, Menu, MenuItem, OsAction, actions};

actions!(
    editor,
    [
        NewProject,
        NewMap,
        OpenWad,
        OpenProject,
        CloseProject,
        SaveProject,
        ImportProject,
        ExportWad,
        Preferences,
        ProjectSettings,
        Remap,
        ChooseMap,
        BuildBsp,
        HealMap,
        AuditMap,
        ShowProject,
        ShowMap,
        ShowTextures,
        ZoomIn,
        ZoomOut,
        ResetView,
        ToggleOverlays,
        ToggleFullscreen,
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = editor, no_json)]
pub(crate) struct OpenRecent(pub String);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = editor, no_json)]
pub(crate) struct ExportPng(pub f32);

pub(crate) fn bind_keys(cx: &mut App) {
    canvas::bind_keys(cx);
    texture_editor::bind_keys(cx);
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    macro_rules! binding {
        ($key:literal, $action:expr) => {
            KeyBinding::new(&format!("{modifier}-{}", $key), $action, Some("Editor"))
        };
    }
    cx.bind_keys([
        binding!("n", NewProject),
        binding!("shift-n", NewMap),
        binding!("o", OpenWad),
        binding!("shift-o", OpenProject),
        binding!("s", SaveProject),
        binding!("w", CloseProject),
        binding!(",", Preferences),
        binding!("1", ShowProject),
        binding!("2", ShowMap),
        binding!("3", ShowTextures),
        binding!("c", ui_common::Copy),
        binding!("x", ui_common::Cut),
        binding!("v", ui_common::Paste),
        binding!("z", ui_common::Undo),
        binding!("shift-z", ui_common::Redo),
        binding!("y", ui_common::Redo),
    ]);
}

pub(crate) fn install(recent: &[String], busy: bool, has_map: bool, cx: &App) {
    let recent = MenuItem::submenu(
        Menu::new("Open Recent").items(
            recent
                .iter()
                .map(|path| MenuItem::action(path.clone(), OpenRecent(path.clone()))),
        ),
    );
    cx.set_menus([
        Menu::new("Room4Doom Editor").items([
            MenuItem::action("Preferences…", Preferences),
            MenuItem::separator(),
            MenuItem::action("Quit Room4Doom Editor", ui_common::Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Project…", NewProject),
            MenuItem::action("New Map…", NewMap),
            MenuItem::separator(),
            MenuItem::action("Open WAD…", OpenWad),
            MenuItem::action("Open Project…", OpenProject),
            recent,
            MenuItem::separator(),
            MenuItem::action("Save Project", SaveProject),
            MenuItem::action("Import DoomEd Project…", ImportProject),
            MenuItem::submenu(Menu::new("Export").items([
                MenuItem::action("WAD…", ExportWad).disabled(busy || !has_map),
                MenuItem::submenu(Menu::new("PNG").items([
                    MenuItem::action("0.5×", ExportPng(0.5)).disabled(busy || !has_map),
                    MenuItem::action("1×", ExportPng(1.0)).disabled(busy || !has_map),
                    MenuItem::action("2×", ExportPng(2.0)).disabled(busy || !has_map),
                ])),
            ])),
            MenuItem::separator(),
            MenuItem::action("Close Project", CloseProject),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", ui_common::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", ui_common::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", ui_common::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", ui_common::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", ui_common::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", ui_common::SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::action("Project Settings…", ProjectSettings),
            MenuItem::action("Remap…", Remap),
        ]),
        Menu::new("Map").items([
            MenuItem::action("Choose Map…", ChooseMap),
            MenuItem::action("Build BSP…", BuildBsp).disabled(busy || !has_map),
            MenuItem::action("Heal Geometry", HealMap).disabled(!has_map),
            MenuItem::action("Audit Map…", AuditMap).disabled(!has_map),
        ]),
        Menu::new("View").items([
            MenuItem::action("Project", ShowProject),
            MenuItem::action("Map", ShowMap),
            MenuItem::action("Texture Editor", ShowTextures),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn).disabled(!has_map),
            MenuItem::action("Zoom Out", ZoomOut).disabled(!has_map),
            MenuItem::action("Reset View", ResetView).disabled(!has_map),
            MenuItem::action("Toggle Overlays", ToggleOverlays).disabled(!has_map),
            MenuItem::separator(),
            MenuItem::action("Toggle Full Screen", ToggleFullscreen),
        ]),
    ]);
}
