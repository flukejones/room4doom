//! Preferences dialog: populate from `EditorPrefs`, apply theme live, persist on Apply.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rbsp::wad_io::NodesFormat;
use slint::{ComponentHandle as _, VecModel};

use crate::generated::{EditorWindow, PrefsController};
use crate::level_editor::preview::push_preview_mode;
use crate::prefs::{self, PopupWindow, SectorGradient, ThemeMode};
use crate::render::apply_damage;
use crate::state::{Damage, SharedState};
use crate::theme;
use crate::views::view_chrome::push_theme;
use crate::views::view_window::restore as restore_geom;
use opaline::ThemeVariant;

/// Theme id at combo `index` for `variant`; default when out of range.
fn theme_id_at(variant: ThemeVariant, index: i32) -> String {
    let themes = theme::themes_for(variant);
    themes
        .into_iter()
        .nth(index.max(0) as usize)
        .map(|(id, _)| id)
        .unwrap_or_else(|| match variant {
            ThemeVariant::Light => theme::DEFAULT_LIGHT_THEME.to_owned(),
            ThemeVariant::Dark => theme::DEFAULT_DARK_THEME.to_owned(),
        })
}

fn theme_labels(variant: ThemeVariant) -> Vec<slint::SharedString> {
    theme::themes_for(variant)
        .into_iter()
        .map(|(_, display)| slint::SharedString::from(display))
        .collect()
}

fn theme_index(variant: ThemeVariant, id: &str) -> i32 {
    theme::themes_for(variant)
        .iter()
        .position(|(tid, _)| tid == id)
        .unwrap_or(0) as i32
}

/// Combo index → `ThemeMode`; out-of-range falls back to Auto.
fn theme_mode_from_index(index: i32) -> ThemeMode {
    match index {
        0 => ThemeMode::Auto,
        1 => ThemeMode::Light,
        2 => ThemeMode::Dark,
        other => {
            log::warn!("unexpected theme-mode index {other}; using Auto");
            ThemeMode::Auto
        }
    }
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<PrefsController>();
    ctl.set_light_themes(slint::ModelRc::new(VecModel::from(theme_labels(
        ThemeVariant::Light,
    ))));
    ctl.set_dark_themes(slint::ModelRc::new(VecModel::from(theme_labels(
        ThemeVariant::Dark,
    ))));

    let gradients: Vec<slint::SharedString> = SectorGradient::ALL
        .iter()
        .map(|g| slint::SharedString::from(g.label()))
        .collect();
    ui.global::<PrefsController>()
        .set_sector_gradients(slint::ModelRc::new(VecModel::from(gradients)));

    // Snapshot at open so Cancel can revert live changes.
    let theme_on_open = Rc::new(Cell::new(ThemeMode::default()));
    let glass_on_open = Rc::new(Cell::new(0.0_f32));

    let weak = ui.as_weak();
    let s = shared.clone();
    let on_open = theme_on_open.clone();
    let glass_open = glass_on_open.clone();
    ui.global::<PrefsController>().on_populate_prefs(move || {
        let Some(ui) = weak.upgrade() else { return };
        on_open.set(s.borrow().prefs.theme_mode);
        glass_open.set(s.borrow().prefs.window_glass_alpha);
        restore_geom(&ui, &s, PopupWindow::Prefs);
        populate(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<PrefsController>()
        .on_glass_alpha_changed(move |alpha| {
            let Some(ui) = weak.upgrade() else { return };
            s.borrow_mut().prefs.window_glass_alpha = alpha.clamp(0.0, 1.0);
            push_theme(&ui, &s);
            apply_damage(&ui, &s, Damage::Repaint);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<PrefsController>()
        .on_light_theme_selected(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            s.borrow_mut().prefs.light_theme = theme_id_at(ThemeVariant::Light, index);
            push_theme(&ui, &s);
            // Line/vertex colours baked into mesh; rebuild required.
            apply_damage(&ui, &s, Damage::Geometry);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<PrefsController>()
        .on_dark_theme_selected(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            s.borrow_mut().prefs.dark_theme = theme_id_at(ThemeVariant::Dark, index);
            push_theme(&ui, &s);
            apply_damage(&ui, &s, Damage::Geometry);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<PrefsController>()
        .on_theme_mode_selected(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let state = &mut *s.borrow_mut();
                state.prefs.theme_mode = theme_mode_from_index(index);
            }
            push_theme(&ui, &s);
            apply_damage(&ui, &s, Damage::Geometry);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<PrefsController>()
        .on_sector_gradient_selected(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            let g = SectorGradient::ALL
                .get(index as usize)
                .copied()
                .unwrap_or_default();
            s.borrow_mut().prefs.sector_gradient = g;
            apply_damage(&ui, &s, Damage::Geometry);
        });

    let weak = ui.as_weak();
    ui.global::<PrefsController>().on_pick_engine_path(move || {
        if let Some(path) = rfd::FileDialog::new().pick_file()
            && let Some(ui) = weak.upgrade()
        {
            ui.global::<PrefsController>()
                .set_engine_path(path.display().to_string().into());
        }
    });

    let weak = ui.as_weak();
    ui.global::<PrefsController>().on_pick_iwad_path(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WAD", &["wad", "WAD"])
            .pick_file()
            && let Some(ui) = weak.upgrade()
        {
            ui.global::<PrefsController>()
                .set_iwad_path(path.display().to_string().into());
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let on_open = theme_on_open.clone();
    let glass_open = glass_on_open.clone();
    ui.global::<PrefsController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
        // Applied; advance snapshot so close won't revert.
        on_open.set(s.borrow().prefs.theme_mode);
        glass_open.set(s.borrow().prefs.window_glass_alpha);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    let on_open = theme_on_open;
    let glass_open = glass_on_open;
    ui.global::<PrefsController>().on_prefs_closed(move || {
        let Some(ui) = weak.upgrade() else { return };
        // Cancel: revert live theme/glass to snapshot.
        let revert = {
            let state = &mut *s.borrow_mut();
            let changed = state.prefs.theme_mode != on_open.get()
                || state.prefs.window_glass_alpha != glass_open.get();
            state.prefs.theme_mode = on_open.get();
            state.prefs.window_glass_alpha = glass_open.get();
            changed
        };
        if revert {
            push_theme(&ui, &s);
            apply_damage(&ui, &s, Damage::Repaint);
        }
    });
}

pub(crate) fn push_toolbar_position(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let index = match shared.borrow().prefs.toolbar_position {
        prefs::ToolbarPositionPref::Top => 0,
        prefs::ToolbarPositionPref::Left => 1,
    };
    ui.set_toolbar_position(index);
}

fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<PrefsController>();
    let prefs = &shared.borrow().prefs;
    ctl.set_engine_path(prefs.engine_path.clone().into());
    ctl.set_iwad_path(prefs.iwad.display().to_string().into());
    ctl.set_launch_type(slint::format!("{}", prefs.launch_type));
    ctl.set_grid(slint::format!("{}", prefs.grid));
    ctl.set_nodes_format_index(match prefs.nodes_format {
        NodesFormat::Room4Doom => 0,
        NodesFormat::Classic => 1,
        NodesFormat::Both => 2,
    });
    ctl.set_bsp_anim_index(match prefs.bsp_anim {
        prefs::BspAnimPref::Off => 0,
        prefs::BspAnimPref::Instant => 1,
        prefs::BspAnimPref::Timed => 2,
    });
    ctl.set_light_anim(prefs.light_anim);
    ctl.set_toolbar_position_index(match prefs.toolbar_position {
        prefs::ToolbarPositionPref::Top => 0,
        prefs::ToolbarPositionPref::Left => 1,
    });
    ctl.set_preview_mode_index(match prefs.preview_mode {
        prefs::PreviewMode::HoverDelayed => 0,
        prefs::PreviewMode::OnClick => 1,
        prefs::PreviewMode::PinnedCorner => 2,
    });
    ctl.set_preview_delay_ms(slint::format!("{}", prefs.preview_hover_delay_ms));
    ctl.set_wp_min_w(slint::format!("{}", prefs.wall_preview_min_w));
    ctl.set_wp_min_h(slint::format!("{}", prefs.wall_preview_min_h));
    ctl.set_wp_max_w(slint::format!("{}", prefs.wall_preview_max_w));
    ctl.set_wp_max_h(slint::format!("{}", prefs.wall_preview_max_h));

    ctl.set_theme_mode_index(match prefs.theme_mode {
        ThemeMode::Auto => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    });
    let gi = SectorGradient::ALL
        .iter()
        .position(|g| *g == prefs.sector_gradient)
        .unwrap_or(0);
    ctl.set_sector_gradient_index(gi as i32);
    ctl.set_glass_supported(cfg!(target_os = "macos"));
    ctl.set_glass_alpha(prefs.window_glass_alpha.clamp(0.0, 1.0));
    ctl.set_light_theme_index(theme_index(ThemeVariant::Light, &prefs.light_theme));
    ctl.set_dark_theme_index(theme_index(ThemeVariant::Dark, &prefs.dark_theme));
}

fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<PrefsController>();
    let damage = {
        let state = &mut *shared.borrow_mut();
        let prefs = &mut state.prefs;
        prefs.engine_path = ctl.get_engine_path().to_string();
        prefs.iwad = ctl.get_iwad_path().to_string().into();
        prefs.launch_type = ctl
            .get_launch_type()
            .trim()
            .parse()
            .unwrap_or(prefs.launch_type);
        prefs.grid = ctl.get_grid().trim().parse().unwrap_or(prefs.grid);
        prefs.nodes_format = match ctl.get_nodes_format_index() {
            0 => NodesFormat::Room4Doom,
            1 => NodesFormat::Classic,
            2 => NodesFormat::Both,
            other => {
                log::warn!("unexpected nodes-format index {other}; using Both");
                NodesFormat::Both
            }
        };
        prefs.bsp_anim = match ctl.get_bsp_anim_index() {
            0 => prefs::BspAnimPref::Off,
            1 => prefs::BspAnimPref::Instant,
            2 => prefs::BspAnimPref::Timed,
            other => {
                log::warn!("unexpected bsp-anim index {other}; using Timed");
                prefs::BspAnimPref::Timed
            }
        };
        prefs.light_anim = ctl.get_light_anim();
        prefs.toolbar_position = match ctl.get_toolbar_position_index() {
            0 => prefs::ToolbarPositionPref::Top,
            1 => prefs::ToolbarPositionPref::Left,
            other => {
                log::warn!("unexpected toolbar-position index {other}; using Left");
                prefs::ToolbarPositionPref::Left
            }
        };
        prefs.preview_mode = match ctl.get_preview_mode_index() {
            0 => prefs::PreviewMode::HoverDelayed,
            1 => prefs::PreviewMode::OnClick,
            2 => prefs::PreviewMode::PinnedCorner,
            other => {
                log::warn!("unexpected preview-mode index {other}; using PinnedCorner");
                prefs::PreviewMode::PinnedCorner
            }
        };
        let f = |s: slint::SharedString, fallback: f32| s.trim().parse().unwrap_or(fallback);
        prefs.preview_hover_delay_ms = f(ctl.get_preview_delay_ms(), prefs.preview_hover_delay_ms);
        prefs.wall_preview_min_w = f(ctl.get_wp_min_w(), prefs.wall_preview_min_w);
        prefs.wall_preview_min_h = f(ctl.get_wp_min_h(), prefs.wall_preview_min_h);
        prefs.wall_preview_max_w = f(ctl.get_wp_max_w(), prefs.wall_preview_max_w);
        prefs.wall_preview_max_h = f(ctl.get_wp_max_h(), prefs.wall_preview_max_h);

        prefs.theme_mode = theme_mode_from_index(ctl.get_theme_mode_index());
        prefs.sector_gradient = SectorGradient::ALL
            .get(ctl.get_sector_gradient_index() as usize)
            .copied()
            .unwrap_or_default();
        prefs.window_glass_alpha = ctl.get_glass_alpha().clamp(0.0, 1.0);
        prefs.light_theme = theme_id_at(ThemeVariant::Light, ctl.get_light_theme_index());
        prefs.dark_theme = theme_id_at(ThemeVariant::Dark, ctl.get_dark_theme_index());

        state.app.grid = state.prefs.grid.max(1);
        if let Err(e) = prefs::save_prefs(&state.prefs) {
            log::warn!("saving prefs: {e}");
        }
        // Line/vertex colours baked; mesh rebuild required.
        Damage::Geometry
    };
    push_toolbar_position(ui, shared);
    push_preview_mode(ui, shared);
    push_theme(ui, shared);
    apply_damage(ui, shared, damage);
}
