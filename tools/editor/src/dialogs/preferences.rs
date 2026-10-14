//! Editor behavior, themes and appearance preferences.

pub(crate) mod actions;

use super::*;
use crate::app::preferences::{
    BspAnimPref, EditorPreferences, PreviewMode, SectorGradient, ThemeMode, ToolbarPositionPref,
};
use editor_core::NodesFormat;
use gpui::{Div, ElementId, SharedString};
use std::rc::Rc;
use ui_common::widgets::{SwitchLabelPosition, Tab, TabBar, TabPosition};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum PreferencesTab {
    Settings,
    Controls,
}

#[derive(Clone, Default)]
pub(crate) struct PreferencesDraft {
    pub(crate) engine_path: String,
    pub(crate) iwad_path: String,
    pub(crate) launch_type: i32,
    pub(crate) grid: i32,
    pub(crate) wp_min_w: String,
    pub(crate) wp_min_h: String,
    pub(crate) wp_max_w: String,
    pub(crate) wp_max_h: String,
    pub(crate) nodes_format: NodesFormat,
    pub(crate) bsp_anim: BspAnimPref,
    pub(crate) light_anim: bool,
    pub(crate) toolbar_position: ToolbarPositionPref,
    pub(crate) preview_mode: PreviewMode,
    pub(crate) preview_delay_ms: String,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) sector_gradient: SectorGradient,
    pub(crate) glass_supported: bool,
    pub(crate) glass_enabled: bool,
    pub(crate) glass_alpha: f32,
    pub(crate) launch_choices: Rc<[(i32, SharedString)]>,
    pub(crate) grid_choices: Rc<[(i32, SharedString)]>,
    pub(crate) light_themes: Rc<[(SharedString, SharedString)]>,
    pub(crate) dark_themes: Rc<[(SharedString, SharedString)]>,
    pub(crate) light_theme: SharedString,
    pub(crate) dark_theme: SharedString,
    pub(crate) snapshot: AppearanceSnapshot,
}

#[derive(Clone, Default, PartialEq)]
pub(crate) struct AppearanceSnapshot {
    theme_mode: ThemeMode,
    light_theme: String,
    dark_theme: String,
    sector_gradient: SectorGradient,
    glass_enabled: bool,
    glass_alpha: f32,
}

impl AppearanceSnapshot {
    pub(crate) fn capture(prefs: &EditorPreferences) -> Self {
        Self {
            theme_mode: prefs.theme_mode,
            light_theme: prefs.light_theme.clone(),
            dark_theme: prefs.dark_theme.clone(),
            sector_gradient: prefs.sector_gradient,
            glass_enabled: prefs.window_glass_enabled,
            glass_alpha: prefs.window_glass_alpha,
        }
    }
    pub(crate) fn restore(&self, prefs: &mut EditorPreferences) {
        prefs.theme_mode = self.theme_mode;
        prefs.light_theme.clone_from(&self.light_theme);
        prefs.dark_theme.clone_from(&self.dark_theme);
        prefs.sector_gradient = self.sector_gradient;
        prefs.window_glass_enabled = self.glass_enabled;
        prefs.window_glass_alpha = self.glass_alpha;
    }
}

impl DialogsView {
    pub(super) fn preferences(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs = TabBar::new("preferences-tabs").children(
            [
                (PreferencesTab::Settings, "Settings"),
                (PreferencesTab::Controls, "Controls"),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (tab, label))| {
                let view = cx.weak_entity();
                let inputs = self.forms.inputs.clone();
                let focus = self.popup_focus.clone();
                Tab::new(if index == 0 {
                    "preferences-settings-tab"
                } else {
                    "preferences-controls-tab"
                })
                .debug_selector(move || {
                    if index == 0 {
                        "preferences-settings-tab"
                    } else {
                        "preferences-controls-tab"
                    }
                    .into()
                })
                .position(if index == 0 {
                    TabPosition::First
                } else {
                    TabPosition::Last
                })
                .toggle_state(tab == self.preferences_tab)
                .child(Label::new(label))
                .on_click(move |_, window, cx| {
                    flush_input_registry(&inputs, window, cx);
                    window.focus(&focus, cx);
                    _ = view.update(cx, |this, cx| {
                        this.preferences_tab = tab;
                        cx.notify();
                    });
                })
            }),
        );
        let (index, id, content) = match self.preferences_tab {
            PreferencesTab::Settings => (
                0,
                "preferences-settings",
                self.preferences_settings(window, cx),
            ),
            PreferencesTab::Controls => (1, "preferences-controls", controls()),
        };
        div()
            .flex()
            .flex_col()
            .h_0()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .flex_none()
                    .debug_selector(|| "preferences-tabs".into())
                    .child(tabs),
            )
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.preferences_scroll[index])
                    .pt_2()
                    .child(content),
            )
            .into_any_element()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Declare the preferences form in display order."
    )]
    fn preferences_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let draft = &self.drafts.prefs;
        let mut panel = div().flex().flex_col().flex_none().w_full().gap_2();
        macro_rules! field {
            ($field:ident, $label:literal $(, $browse:literal, $path:ident)?) => {
                let input = self.forms.field(
                    concat!("prefs-", stringify!($field)),
                    $label,
                    &draft.$field,
                    cx.listener(|this, value: &String, _, cx| {
                        this.drafts.prefs.$field.clone_from(value);
                        cx.notify();
                    }),
                    |_, _, _| {},
                    window,
                    cx,
                );
                let row = div().flex().flex_none().w_full().items_center().gap_2()
                    .child(div().flex_1().min_w_0().child(input));
                $(let row = row.child(self.forms.button(
                    $browse, "Browse…", cx.listener(|this, _, _, cx| {
                        this.pick_path(PathField::$path, cx);
                    }),
                ));)?
                panel = panel.child(row);
            };
        }
        field!(engine_path, "Engine", "pick-engine", Engine);
        field!(iwad_path, "IWAD", "pick-iwad", Iwad);
        panel = panel.child(self.preference_choice(
            "prefs-launch-type",
            "Launch type",
            draft.launch_choices.clone(),
            &draft.launch_type,
            cx.listener(|this, kind: &i32, _, cx| {
                this.drafts.prefs.launch_type = *kind;
                cx.notify();
            }),
            window,
            cx,
        ));
        panel = panel.child(self.preference_choice(
            "prefs-grid",
            "Grid",
            draft.grid_choices.clone(),
            &draft.grid,
            cx.listener(|this, grid: &i32, _, cx| {
                this.drafts.prefs.grid = *grid;
                cx.notify();
            }),
            window,
            cx,
        ));
        macro_rules! choices {
            ($field:ident, $label:literal, $type:ty, $options:expr) => {
                panel = panel.child(self.preference_choice(
                    concat!("prefs-", stringify!($field)),
                    $label,
                    $options,
                    &draft.$field,
                    cx.listener(|this, value: &$type, _, cx| {
                        this.drafts.prefs.$field = *value;
                        cx.notify();
                    }),
                    window,
                    cx,
                ));
            };
        }
        choices!(
            nodes_format,
            "Nodes",
            NodesFormat,
            [
                (NodesFormat::Room4Doom, "Room4Doom"),
                (NodesFormat::Classic, "Classic"),
                (NodesFormat::Both, "Both"),
            ]
        );
        choices!(
            bsp_anim,
            "Build animation",
            BspAnimPref,
            [
                (BspAnimPref::Off, "Off"),
                (BspAnimPref::Instant, "Instant"),
                (BspAnimPref::Timed, "Timed"),
            ]
        );
        panel = panel.child(preference_row(
            "Animate lights",
            self.forms
                .switch(
                    "prefs-lights",
                    "Animate lights",
                    draft.light_anim,
                    cx.listener(|this, value: &bool, _, cx| {
                        this.drafts.prefs.light_anim = *value;
                        cx.notify();
                    }),
                )
                .label_position(None::<SwitchLabelPosition>),
        ));
        choices!(
            toolbar_position,
            "Toolbar",
            ToolbarPositionPref,
            [
                (ToolbarPositionPref::Top, "Top"),
                (ToolbarPositionPref::Left, "Left"),
            ]
        );
        choices!(
            preview_mode,
            "Preview",
            PreviewMode,
            [
                (PreviewMode::HoverDelayed, "Hover (delayed)"),
                (PreviewMode::OnClick, "On click"),
                (PreviewMode::PinnedCorner, "Pinned corner"),
            ]
        );
        field!(preview_delay_ms, "Hover delay ms");
        panel = panel.child(heading("Wall preview size"));
        field!(wp_min_w, "Minimum width");
        field!(wp_min_h, "Minimum height");
        field!(wp_max_w, "Maximum width");
        field!(wp_max_h, "Maximum height");
        panel = panel.child(heading("Appearance"));
        panel = panel.child(self.preference_choice(
            "prefs-theme-mode",
            "Theme mode",
            [
                (ThemeMode::Auto, "Auto (system)"),
                (ThemeMode::Light, "Light"),
                (ThemeMode::Dark, "Dark"),
            ],
            &draft.theme_mode,
            cx.listener(|this, mode: &ThemeMode, _, cx| {
                this.drafts.prefs.theme_mode = *mode;
                this.preview_preferences(cx);
            }),
            window,
            cx,
        ));
        for (light, label, selected, themes) in [
            (true, "Light theme", &draft.light_theme, &draft.light_themes),
            (false, "Dark theme", &draft.dark_theme, &draft.dark_themes),
        ] {
            panel = panel.child(self.preference_choice(
                if light {
                    "prefs-light-theme"
                } else {
                    "prefs-dark-theme"
                },
                label,
                themes.clone(),
                selected,
                cx.listener(move |this, id: &SharedString, _, cx| {
                    if light {
                        this.drafts.prefs.light_theme.clone_from(id);
                    } else {
                        this.drafts.prefs.dark_theme.clone_from(id);
                    }
                    this.preview_preferences(cx);
                }),
                window,
                cx,
            ));
        }
        panel = panel.child(self.preference_choice(
            "prefs-sector-gradient",
            "Sector colours",
            SectorGradient::ALL.map(|gradient| (gradient, gradient.label())),
            &draft.sector_gradient,
            cx.listener(|this, gradient: &SectorGradient, _, cx| {
                this.drafts.prefs.sector_gradient = *gradient;
                this.preview_preferences(cx);
            }),
            window,
            cx,
        ));
        if draft.glass_supported {
            panel = panel.child(preference_row(
                "Glass background",
                self.forms
                    .switch(
                        "prefs-glass-enabled",
                        "Glass background",
                        draft.glass_enabled,
                        cx.listener(|this, value: &bool, _, cx| {
                            this.drafts.prefs.glass_enabled = *value;
                            this.preview_preferences(cx);
                        }),
                    )
                    .label_position(None::<SwitchLabelPosition>),
            ));
            if draft.glass_enabled {
                panel = panel.child(self.preference_choice(
                    "prefs-glass",
                    "Window opacity",
                    [
                        (0., "0%"),
                        (0.1, "10%"),
                        (0.2, "20%"),
                        (0.3, "30%"),
                        (0.4, "40%"),
                        (0.5, "50%"),
                        (0.6, "60%"),
                        (0.7, "70%"),
                        (0.8, "80%"),
                        (0.9, "90%"),
                        (1., "100%"),
                    ],
                    &((draft.glass_alpha * 10.).round() / 10.),
                    cx.listener(|this, alpha: &f32, _, cx| {
                        this.drafts.prefs.glass_alpha = *alpha;
                        this.preview_preferences(cx);
                    }),
                    window,
                    cx,
                ));
            }
        }
        panel.into_any_element()
    }

    fn preference_choice<T: PartialEq + 'static, S: Clone + Into<SharedString> + 'static>(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl AsRef<[(T, S)]> + Clone + 'static,
        selected: &T,
        callback: impl Fn(&T, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        preference_row(
            label,
            self.forms
                .typed_choices(id, options, Some(selected), callback, window, cx)
                .full_width(true),
        )
        .into_any_element()
    }
}
fn preference_row(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_none()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .child(Label::new(label).single_line()),
        )
        .child(div().flex_1().min_w_0().child(control))
}

fn controls() -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .gap_4()
        .child(controls_section(
            "Mouse",
            &[
                ("Use the current tool", "Left click"),
                ("Add / remove selection", "Cmd/Ctrl + click"),
                ("Drag with the current tool", "Left drag"),
                ("Edit a wall or sector", "Double-click"),
                ("Context menu", "Right click"),
                ("Pan", "Middle / Alt + drag"),
                ("Pan", "Two-finger scroll"),
                ("Orbit", "Shift + drag / scroll"),
                ("Zoom", "Wheel / Ctrl + scroll / pinch"),
            ],
            false,
        ))
        .child(controls_section(
            "Map keyboard",
            &[
                ("Raise / lower selected surface", "↑ / ↓"),
                ("Raise / lower opposing surface", "Shift + ↑ / ↓"),
                ("Toggle overlays", "Space"),
                ("Cycle grid size", "G"),
                ("Flip selected lines", "F"),
                ("Split intersections", "I"),
                ("Pick brush from selection", "B"),
                ("Zoom in / out", "+ or = / −"),
                ("Reset view", "3"),
                ("Commit drawing or drag choice", "Enter"),
                ("Cancel / clear selection", "Escape"),
                ("Delete selection", "Delete / Backspace"),
                ("Choose Move or Extrude", "↑ / ↓ in drag choice"),
            ],
            false,
        ))
        .child(controls_section(
            "Texture editor",
            &[
                ("Move selected patch by 1 unit", "Arrow keys"),
                ("Move selected patch by 10 units", "Shift + arrows"),
                ("Previous / next texture", "↑ / ↓ without a selected patch"),
            ],
            false,
        ))
        .child(controls_section(
            "Wall editor",
            &[
                ("Adjust texture offsets", "Arrow keys"),
                ("Adjust offsets with a larger step", "Shift + arrows"),
            ],
            false,
        ))
        .child(controls_section(
            "Application",
            &[
                ("New project", "N"),
                ("New map", "Shift + N"),
                ("Open WAD", "O"),
                ("Open project", "Shift + O"),
                ("Save project", "S"),
                ("Close project", "W"),
                ("Preferences", ","),
                ("Project / Map / Texture Editor", "1 / 2 / 3"),
                ("Undo", "Z"),
                ("Redo", "Shift + Z / Y"),
                ("Copy / cut / paste", "C / X / V"),
                ("Quit", "Q"),
            ],
            true,
        ))
        .child(controls_section(
            "Fields and dialogs",
            &[
                ("Accept field", "Enter"),
                ("Next / previous control", "Tab / Shift + Tab"),
                ("Cancel dialog", "Escape"),
            ],
            false,
        ))
        .into_any_element()
}

fn controls_section(
    title: &'static str,
    bindings: &'static [(&'static str, &'static str)],
    primary_modifier: bool,
) -> Div {
    let modifier = if cfg!(target_os = "macos") {
        "Cmd + "
    } else {
        "Ctrl + "
    };
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(heading(title))
        .children(bindings.iter().map(|(action, keys)| {
            div()
                .flex()
                .flex_none()
                .w_full()
                .items_start()
                .gap_3()
                .child(div().flex_1().min_w_0().child(Label::new(*action)))
                .child(
                    div()
                        .w(px(200.))
                        .flex_shrink_0()
                        .flex()
                        .flex_wrap()
                        .when(primary_modifier, |key| {
                            key.child(Label::new(modifier).color(Color::Muted))
                        })
                        .child(Label::new(*keys).color(Color::Muted)),
                )
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_editor::LevelEditorState;
    use gpui::{
        Focusable as _, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext,
        VisualTestContext, point, size,
    };
    use std::cell::RefCell;

    #[gpui::test]
    fn tabs_preserve_drafts_and_keep_navigation_outside_the_scrolling_list(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| ui_common::init("Preferences test", cx));
        let shared = Rc::new(RefCell::new(EditorSession::new(
            LevelEditorState::new(),
            None,
            Default::default(),
        )));
        let saved_engine = shared.borrow().prefs.engine_path.clone();
        let root = cx.open_window(size(px(640.), px(400.)), |_, cx| {
            let mut view = DialogsView::new(shared.clone(), cx);
            actions::begin(&mut view.drafts.prefs, &shared.borrow());
            view.active = Some(PopupId::Prefs);
            view.native_title = "Preferences".into();
            view
        });
        let input = root
            .update(cx, |view, window, cx| {
                let input = view.forms.inputs.borrow()["prefs-engine_path"].clone();
                window.focus(&input.focus_handle(cx), cx);
                input
            })
            .unwrap();
        cx.dispatch_action(root.into(), ui_common::SelectAll);
        cx.simulate_input(root.into(), "room4doom-test");
        let mut visual = VisualTestContext::from_window(root.into(), cx);
        let tabs = visual.debug_bounds("preferences-tabs").unwrap();
        let controls = visual.debug_bounds("preferences-controls-tab").unwrap();
        visual.simulate_click(controls.center(), Modifiers::none());
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let scroll = root
            .update(&mut visual, |view, _, _| view.preferences_scroll[1].clone())
            .unwrap();
        let bounds = visual.debug_bounds("preferences-controls").unwrap();
        assert!(bounds.top() >= tabs.bottom());
        assert!(
            bounds.bottom() < px(400.),
            "the list must leave room for the footer: {bounds:?}, tabs: {tabs:?}, dialog: {:?}",
            visual.debug_bounds("editor-dialog")
        );
        visual.simulate_event(ScrollWheelEvent {
            position: bounds.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-500.))),
            ..Default::default()
        });
        visual.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            scroll.offset().y < px(0.),
            "the controls reference must scroll"
        );
        assert_eq!(visual.debug_bounds("preferences-tabs").unwrap(), tabs);
        let settings = visual.debug_bounds("preferences-settings-tab").unwrap();
        visual.simulate_click(settings.center(), Modifiers::none());
        visual.update(|window, cx| window.draw(cx).clear(cx));
        root.update(&mut visual, |view, _, cx| {
            assert_eq!(view.drafts.prefs.engine_path, "room4doom-test");
            assert_eq!(input.read(cx).text(), "room4doom-test");
        })
        .unwrap();
        assert_eq!(
            shared.borrow().prefs.engine_path,
            saved_engine,
            "switching tabs must not apply settings"
        );
    }
}
