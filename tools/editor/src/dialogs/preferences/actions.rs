//! Preferences commands over plain owned draft values.
use super::{AppearanceSnapshot, PreferencesDraft};
use crate::app::preferences as prefs;
use crate::app::session::{Damage, EditorSession};
use crate::defaults::launch_type_choices;
use crate::theme;
use gpui::SharedString;
use opaline::ThemeVariant;

fn theme_choices(
    variant: ThemeVariant,
    selected: &str,
) -> std::rc::Rc<[(SharedString, SharedString)]> {
    let mut choices: Vec<_> = theme::themes_for(variant)
        .into_iter()
        .map(|(id, label)| (SharedString::from(id), SharedString::from(label)))
        .collect();
    if !choices.iter().any(|(id, _)| id.as_ref() == selected) {
        let selected: SharedString = selected.to_owned().into();
        choices.push((selected.clone(), selected));
    }
    choices.into()
}

pub(crate) fn populate(draft: &mut PreferencesDraft, state: &EditorSession) {
    const GRIDS: [i32; 7] = [1, 2, 4, 8, 16, 32, 64];
    let prefs = &state.prefs;
    draft.light_themes = theme_choices(ThemeVariant::Light, &prefs.light_theme);
    draft.dark_themes = theme_choices(ThemeVariant::Dark, &prefs.dark_theme);
    draft.engine_path.clone_from(&prefs.engine_path);
    draft.iwad_path = prefs.iwad.display().to_string();
    draft.launch_type = prefs.launch_type;
    draft.launch_choices = launch_type_choices(prefs.launch_type);
    draft.grid_choices = GRIDS
        .into_iter()
        .chain((!GRIDS.contains(&prefs.grid)).then_some(prefs.grid))
        .map(|grid| (grid, grid.to_string().into()))
        .collect();
    draft.grid = prefs.grid;
    draft.nodes_format = prefs.nodes_format;
    draft.bsp_anim = prefs.bsp_anim;
    draft.light_anim = prefs.light_anim;
    draft.toolbar_position = prefs.toolbar_position;
    draft.preview_mode = prefs.preview_mode;
    draft.preview_delay_ms = prefs.preview_hover_delay_ms.to_string();
    draft.wp_min_w = prefs.wall_preview_min_w.to_string();
    draft.wp_min_h = prefs.wall_preview_min_h.to_string();
    draft.wp_max_w = prefs.wall_preview_max_w.to_string();
    draft.wp_max_h = prefs.wall_preview_max_h.to_string();
    draft.theme_mode = prefs.theme_mode;
    draft.sector_gradient = prefs.sector_gradient;
    draft.glass_supported = cfg!(target_os = "macos");
    draft.glass_enabled = prefs.window_glass_enabled;
    draft.glass_alpha = prefs.window_glass_alpha.clamp(0.0, 1.0);
    draft.light_theme = prefs.light_theme.clone().into();
    draft.dark_theme = prefs.dark_theme.clone().into();
}

pub(crate) fn apply(draft: &mut PreferencesDraft, state: &mut EditorSession) -> Damage {
    let prefs = &mut state.prefs;
    prefs.engine_path.clone_from(&draft.engine_path);
    prefs.iwad = draft.iwad_path.clone().into();
    prefs.launch_type = draft.launch_type;
    prefs.grid = draft.grid;
    prefs.nodes_format = draft.nodes_format;
    prefs.bsp_anim = draft.bsp_anim;
    prefs.light_anim = draft.light_anim;
    prefs.toolbar_position = draft.toolbar_position;
    prefs.preview_mode = draft.preview_mode;
    let number = |text: &str, fallback: f32| text.trim().parse().unwrap_or(fallback);
    prefs.preview_hover_delay_ms = number(&draft.preview_delay_ms, prefs.preview_hover_delay_ms);
    prefs.wall_preview_min_w = number(&draft.wp_min_w, prefs.wall_preview_min_w);
    prefs.wall_preview_min_h = number(&draft.wp_min_h, prefs.wall_preview_min_h);
    prefs.wall_preview_max_w = number(&draft.wp_max_w, prefs.wall_preview_max_w);
    prefs.wall_preview_max_h = number(&draft.wp_max_h, prefs.wall_preview_max_h);
    preview(draft, state);
    state.map_editor.grid = state.prefs.grid.max(1);
    if let Err(error) = prefs::save_prefs(&state.prefs) {
        log::warn!("saving prefs: {error}");
    }
    draft.snapshot = AppearanceSnapshot::capture(&state.prefs);
    Damage::Restyle
}

pub(crate) fn begin(draft: &mut PreferencesDraft, state: &EditorSession) {
    populate(draft, state);
    draft.snapshot = AppearanceSnapshot::capture(&state.prefs);
}

pub(crate) fn preview(draft: &PreferencesDraft, state: &mut EditorSession) -> Damage {
    state.prefs.theme_mode = draft.theme_mode;
    state.prefs.light_theme = draft.light_theme.to_string();
    state.prefs.dark_theme = draft.dark_theme.to_string();
    state.prefs.sector_gradient = draft.sector_gradient;
    state.prefs.window_glass_enabled = draft.glass_enabled;
    state.prefs.window_glass_alpha = draft.glass_alpha.clamp(0., 1.);
    Damage::Restyle
}

pub(crate) fn close(draft: &PreferencesDraft, state: &mut EditorSession) -> Damage {
    let changed = AppearanceSnapshot::capture(&state.prefs) != draft.snapshot;
    draft.snapshot.restore(&mut state.prefs);
    if changed {
        Damage::Restyle
    } else {
        Damage::None
    }
}
