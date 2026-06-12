//! Resolve an `opaline` theme name to Slint chrome colours + map [`CanvasStyle`].

use opaline::{Theme, ThemeInfo, ThemeVariant, list_available_themes, load_by_name};

use crate::render::style::{CanvasStyle, Color};

pub const DEFAULT_LIGHT_THEME: &str = "one_light";
pub const DEFAULT_DARK_THEME: &str = "one_dark";

/// UI chrome colours (RGB) fed to `ThemeController`.
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
    /// ARGB — carries its own alpha.
    pub window_shadow: [u8; 4],
}

/// `(id, display_name)` pairs for built-in themes of `variant`.
pub fn themes_for(variant: ThemeVariant) -> Vec<(String, String)> {
    list_available_themes()
        .into_iter()
        .filter(|t: &ThemeInfo| t.variant == variant)
        .map(|t| (t.name, t.display_name))
        .collect()
}

/// Resolves by name, falling back to the first built-in of `variant`.
fn theme_or_fallback(name: &str, variant: ThemeVariant) -> Theme {
    load_by_name(name)
        .or_else(|| {
            let first = list_available_themes()
                .into_iter()
                .find(|t| t.variant == variant)?;
            load_by_name(&first.name)
        })
        .unwrap_or_default()
}

/// Chrome + canvas colours for `name`.
pub fn resolve(name: &str, variant: ThemeVariant) -> (Chrome, CanvasStyle) {
    let t = theme_or_fallback(name, variant);
    (chrome(&t), canvas(&t))
}

fn rgb(t: &Theme, token: &str) -> Color {
    let c = t.color(token);
    [c.r, c.g, c.b, 0xff]
}

fn chrome(t: &Theme) -> Chrome {
    Chrome {
        window_background: rgb(t, "bg.base"),
        panel_background: rgb(t, "bg.panel"),
        panel_border: rgb(t, "border.unfocused"),
        text: rgb(t, "text.primary"),
        text_dim: rgb(t, "text.muted"),
        accent: rgb(t, "accent.primary"),
        tool_active: rgb(t, "bg.active"),
        tool_hover: rgb(t, "bg.highlight"),
        control_bg: rgb(t, "bg.selection"),
        control_bg_disabled: rgb(t, "bg.code"),
        text_disabled: rgb(t, "text.dim"),
        window_shadow: [0, 0, 0, 0x80],
    }
}

fn canvas(t: &Theme) -> CanvasStyle {
    CanvasStyle {
        back: rgb(t, "bg.base"),
        grid: rgb(t, "bg.highlight"),
        tile: rgb(t, "bg.selection"),
        selected: rgb(t, "code.function"),
        point: rgb(t, "accent.secondary"),
        one_sided: rgb(t, "accent.secondary"),
        two_sided: rgb(t, "accent.primary"),
        special: rgb(t, "accent.tertiary"),
        thing: rgb(t, "accent.deep"),
        warning: rgb(t, "warning"),
    }
}
