//! Adapt the shared GUI theme to the editor's map renderer.

use crate::render::style::CanvasStyle;
use ui_common::theme::{self, ThemeVariant};
pub use ui_common::theme::{Chrome, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, themes_for};

pub fn resolve(name: &str, variant: ThemeVariant) -> (Chrome, CanvasStyle) {
    let (chrome, colors) = theme::resolve(name, variant);
    (
        chrome,
        CanvasStyle {
            back: colors.back,
            grid: colors.grid,
            tile: colors.tile,
            selected: colors.selected,
            point: colors.point,
            one_sided: colors.one_sided,
            two_sided: colors.two_sided,
            special: colors.special,
            thing: colors.thing,
            warning: colors.warning,
        },
    )
}
