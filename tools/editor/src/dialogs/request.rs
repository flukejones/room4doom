//! Typed requests between native feature entities and the application shell.

use super::asset_picker::actions::TexBrowseTarget;
use crate::app::session::Damage;

#[derive(Clone)]
pub(crate) enum DialogRequest {
    Animations,
    Preferences,
    ProjectSettings,
    BuildBsp,
    NewMap,
    ChooseMap,
    Audit,
    Remap,
    Wall(i32),
    Sector(i32),
    AssetPicker {
        target: TexBrowseTarget,
        current: Option<String>,
    },
}

pub(crate) enum DialogsEvent {
    Appearance(gpui::WindowAppearance),
    Quit,
    Animations(Vec<editor_core::AnimDef>),
    Opened,
    Closed,
    Damage(Damage),
    PreferencesChanged,
    ProjectSettingsChanged,
    ChooseMap(String),
    NewMap(String),
    BuildBsp {
        interval_ms: i32,
        keep_all: bool,
    },
    AssetPicked {
        target: TexBrowseTarget,
        name: String,
    },
}
