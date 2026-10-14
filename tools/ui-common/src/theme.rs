//! Shared application and canvas colours from the editor's named themes.

pub use opaline::ThemeVariant;
use opaline::{Theme, ThemeInfo, list_available_themes, load_by_name};
use serde::{Deserialize, Serialize};

pub type Color = [u8; 4];
pub const DEFAULT_LIGHT_THEME: &str = "one-light";
pub const DEFAULT_DARK_THEME: &str = "one-dark";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Auto,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Self::Auto => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

/// Application chrome in RGBA byte order, shared by every GUI.
#[derive(Clone, Debug)]
pub struct Chrome {
    pub window_background: Color,
    pub panel_background: Color,
    pub panel_border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub tool_active: Color,
    pub tool_hover: Color,
    pub control_bg: Color,
    pub control_bg_disabled: Color,
    pub text_disabled: Color,
    pub error: Color,
    pub window_shadow: Color,
}

/// Canvas colours without a dependency on an application's render state.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasColors {
    pub back: Color,
    pub grid: Color,
    pub tile: Color,
    pub selected: Color,
    pub point: Color,
    pub one_sided: Color,
    pub two_sided: Color,
    pub special: Color,
    pub thing: Color,
    pub warning: Color,
}

pub fn themes_for(variant: ThemeVariant) -> Vec<(String, String)> {
    list_available_themes()
        .into_iter()
        .filter(|theme: &ThemeInfo| theme.variant == variant)
        .map(|theme| (theme.name, theme.display_name))
        .collect()
}

fn theme_or_fallback(name: &str, variant: ThemeVariant) -> Theme {
    load_by_name(&name.replace('_', "-"))
        .or_else(|| {
            let first = list_available_themes()
                .into_iter()
                .find(|theme| theme.variant == variant)?;
            load_by_name(&first.name)
        })
        .unwrap_or_default()
}

pub fn resolve(name: &str, variant: ThemeVariant) -> (Chrome, CanvasColors) {
    let theme = theme_or_fallback(name, variant);
    let rgb = |token| {
        let colour = theme.color(token);
        [colour.r, colour.g, colour.b, 0xff]
    };
    (
        Chrome {
            window_background: rgb("bg.base"),
            panel_background: rgb("bg.panel"),
            panel_border: rgb("border.unfocused"),
            text: rgb("text.primary"),
            text_dim: rgb("text.muted"),
            accent: rgb("accent.primary"),
            tool_active: rgb("bg.active"),
            tool_hover: rgb("bg.highlight"),
            control_bg: rgb("bg.selection"),
            control_bg_disabled: rgb("bg.code"),
            text_disabled: rgb("text.dim"),
            error: rgb("error"),
            window_shadow: [0, 0, 0, 0x80],
        },
        CanvasColors {
            back: rgb("bg.base"),
            grid: rgb("bg.highlight"),
            tile: rgb("bg.selection"),
            selected: rgb("code.function"),
            point: rgb("accent.secondary"),
            one_sided: rgb("accent.secondary"),
            two_sided: rgb("accent.primary"),
            special: rgb("accent.tertiary"),
            thing: rgb("accent.deep"),
            warning: rgb("warning"),
        },
    )
}

pub fn system(appearance: gpui::WindowAppearance) -> (Chrome, CanvasColors) {
    if is_dark(appearance) {
        resolve(DEFAULT_DARK_THEME, ThemeVariant::Dark)
    } else {
        resolve(DEFAULT_LIGHT_THEME, ThemeVariant::Light)
    }
}

pub fn is_dark(appearance: gpui::WindowAppearance) -> bool {
    matches!(
        appearance,
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    )
}
