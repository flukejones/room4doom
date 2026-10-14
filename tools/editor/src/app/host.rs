//! Foreground scheduling, native menus and appearance for the application shell.
use super::{
    commands::AppCommand,
    jobs,
    window::{EditorShell, EditorTab},
};
use crate::app::menus::{
    AuditMap, BuildBsp, ChooseMap, CloseProject, ExportPng, ExportWad, HealMap, ImportProject,
    NewMap, NewProject, OpenProject, OpenRecent, OpenWad, Preferences, ProjectSettings, Remap,
    ResetView, SaveProject, ShowMap, ShowProject, ShowTextures, ToggleFullscreen, ToggleOverlays,
    ZoomIn, ZoomOut, bind_keys, install,
};
use crate::dialogs::DialogRequest;
use crate::tabs::{map_editor::MapCommand, texture_editor::EditAction as TextureEditAction};
use gpui::{Context, Div, Subscription, Task, Window, WindowBackgroundAppearance, prelude::*};
use std::time::Instant;
#[derive(Default)]
pub(crate) struct HostTasks {
    timer: Option<ScheduledWake>,
    _jobs: Option<Task<()>>,
    _appearance: Option<Subscription>,
    _activation: Option<Subscription>,
    system_dark: Option<bool>,
    theme: Option<(bool, [[u8; 4]; 13])>,
    glass: Option<u32>,
    menu: Option<(Vec<String>, bool, bool)>,
}
struct ScheduledWake {
    deadline: Instant,
    _task: Task<()>,
}
impl EditorShell {
    pub(crate) fn initialize_native(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        bind_keys(cx);
        self.update_system_appearance(window.appearance(), cx);
        self.native._appearance = Some(cx.observe_window_appearance(window, |this, window, cx| {
            this.update_system_appearance(window.appearance(), cx);
            cx.notify();
        }));
        self.native._activation = Some(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.cancel_canvas_gesture(cx);
                this.cancel_texture_drag(cx);
                this.finish_chrome_drag(cx);
            }
            cx.notify();
        }));
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |view, cx| {
                view.request_quit(window, cx);
                false
            })
            .unwrap_or(true)
        });
        let receiver = self.shared.borrow().job_rx.clone();
        self.native._jobs = Some(cx.spawn(async move |view, cx| {
            while let Ok(outcome) = receiver.recv().await {
                if view
                    .update(cx, |view, cx| {
                        let effect = jobs::finish(&view.shared, outcome);
                        view.job_status = effect.status;
                        if let Some(events) = effect.build_events {
                            let (interval, keep_all) = {
                                let state = view.shared.borrow();
                                (state.anim_interval_ms, state.anim_keep_all)
                            };
                            view.map.update(cx, |map, cx| {
                                map.play_build(events, interval, keep_all, cx);
                            });
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.native_tick(window, cx);
    }
    pub(crate) fn request_quit(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_inputs(window, cx);
        self.focus_commands(window, cx);
        self.dispatch_command(AppCommand::Quit, cx);
    }
    pub(super) fn update_system_appearance(
        &mut self,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        let dark = ui_common::theme::is_dark(appearance);
        if self.native.system_dark == Some(dark) {
            return;
        }
        self.native.system_dark = Some(dark);
        self.appearance.system_dark = dark;
        self.appearance.refresh(&self.shared);
        self.map.update(cx, |map, cx| {
            map.refresh(super::session::Damage::Restyle, cx);
        });
        self.update_widget_theme(cx);
    }
    fn update_widget_theme(&mut self, cx: &mut Context<Self>) {
        let chrome = &self.appearance.chrome;
        let key = (
            self.appearance.dark,
            [
                chrome.window_background,
                chrome.panel_background,
                chrome.panel_border,
                chrome.text,
                chrome.text_dim,
                chrome.accent,
                chrome.tool_active,
                chrome.tool_hover,
                chrome.control_bg,
                chrome.control_bg_disabled,
                chrome.text_disabled,
                chrome.error,
                chrome.window_shadow,
            ],
        );
        if self.native.theme.as_ref() != Some(&key) {
            ui_common::apply_chrome(chrome, self.appearance.dark, cx);
            self.native.theme = Some(key);
        }
    }
    pub(crate) fn native_tick(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.update_system_appearance(window.appearance(), cx);
        self.update_widget_theme(cx);
        let alpha = self.appearance.glass_alpha.clamp(0., 1.);
        if self.native.glass != Some(alpha.to_bits()) {
            window.set_background_appearance(if cfg!(target_os = "macos") && alpha < 1. {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Opaque
            });
            self.native.glass = Some(alpha.to_bits());
        }
        {
            let state = self.shared.borrow();
            let has_map = state.map_editor.map.is_some();
            if self.native.menu.as_ref().is_none_or(|cached| {
                cached.0 != state.prefs.recent_projects
                    || cached.1 != state.job_busy
                    || cached.2 != has_map
            }) {
                install(&state.prefs.recent_projects, state.job_busy, has_map, cx);
                self.native.menu =
                    Some((state.prefs.recent_projects.clone(), state.job_busy, has_map));
            }
        }
        self.schedule_timer(cx);
    }
    fn schedule_timer(&mut self, cx: &Context<Self>) {
        let Some(deadline) = self.map.read(cx).next_deadline() else {
            self.native.timer = None;
            return;
        };
        if self
            .native
            .timer
            .as_ref()
            .is_some_and(|wake| wake.deadline <= deadline)
        {
            return;
        }
        let wait = cx
            .background_executor()
            .timer(deadline.saturating_duration_since(Instant::now()));
        let task = cx.spawn(async move |view, cx| {
            wait.await;
            _ = view.update(cx, |view, cx| {
                view.native.timer = None;
                view.map.update(cx, |map, cx| map.tick(Instant::now(), cx));
                cx.notify();
            });
        });
        self.native.timer = Some(ScheduledWake {
            deadline,
            _task: task,
        });
    }
    pub(crate) fn bind_native_actions(&self, root: Div, cx: &Context<Self>) -> Div {
        macro_rules! command {
            ($root:expr,$action:ty,$command:expr) => {
                $root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    this.flush_inputs(window, cx);
                    this.focus_commands(window, cx);
                    this.dispatch_command($command, cx);
                    cx.stop_propagation();
                }))
            };
        }
        macro_rules! popup {
            ($root:expr,$action:ty,$request:expr) => {
                $root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    this.flush_inputs(window, cx);
                    this.dialogs
                        .update(cx, |dialogs, cx| dialogs.open($request, window, cx));
                    cx.stop_propagation();
                }))
            };
        }
        let root = root.key_context("Editor");
        let root = command!(root, NewProject, AppCommand::NewProject);
        let root = command!(root, OpenWad, AppCommand::OpenWad);
        let root = command!(root, OpenProject, AppCommand::OpenProject);
        let root = command!(root, SaveProject, AppCommand::Save);
        let root = command!(root, CloseProject, AppCommand::CloseProject);
        let root = command!(root, ImportProject, AppCommand::ImportProject);
        let root = command!(root, ExportWad, AppCommand::ExportWad);
        let root = root.on_action(cx.listener(|this, _: &HealMap, window, cx| {
            this.flush_inputs(window, cx);
            this.dialogs.update(cx, |dialogs, cx| dialogs.heal_map(cx));
            cx.stop_propagation();
        }));
        let root = popup!(root, NewMap, DialogRequest::NewMap);
        let root = popup!(root, Preferences, DialogRequest::Preferences);
        let root = popup!(root, ProjectSettings, DialogRequest::ProjectSettings);
        let root = popup!(root, Remap, DialogRequest::Remap);
        let root = popup!(root, ChooseMap, DialogRequest::ChooseMap);
        let root = popup!(root, BuildBsp, DialogRequest::BuildBsp);
        let root = popup!(root, AuditMap, DialogRequest::Audit);
        let root = Self::bind_view_actions(root, cx);
        self.bind_edit_actions(root, cx)
    }
    fn bind_view_actions(root: Div, cx: &Context<Self>) -> Div {
        macro_rules! tab {
            ($root:expr,$action:ty,$tab:ident) => {
                $root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    this.switch_tab(EditorTab::$tab, window, cx)
                }))
            };
        }
        macro_rules! map {
            ($root:expr,$action:ty,$command:ident) => {
                $root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    this.flush_inputs(window, cx);
                    this.map
                        .update(cx, |map, cx| map.command(MapCommand::$command, cx));
                }))
            };
        }
        let root = root
            .on_action(cx.listener(|this, _: &ui_common::Quit, window, cx| {
                this.request_quit(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, action: &OpenRecent, window, cx| {
                this.flush_inputs(window, cx);
                this.focus_commands(window, cx);
                this.dispatch_command(AppCommand::OpenRecent(action.0.clone()), cx);
            }))
            .on_action(cx.listener(|this, action: &ExportPng, window, cx| {
                this.flush_inputs(window, cx);
                this.focus_commands(window, cx);
                this.dispatch_command(AppCommand::ExportPng(action.0), cx);
            }));
        let root = tab!(root, ShowProject, Project);
        let root = tab!(root, ShowMap, Map);
        let root = tab!(root, ShowTextures, Textures);
        let root = map!(root, ZoomIn, ZoomIn);
        let root = map!(root, ZoomOut, ZoomOut);
        let root = map!(root, ResetView, ResetView);
        let root = map!(root, ToggleOverlays, ToggleOverlays);
        root.on_action(cx.listener(|_, _: &ToggleFullscreen, window, _| window.toggle_fullscreen()))
    }
    fn bind_edit_actions(&self, root: Div, cx: &Context<Self>) -> Div {
        if !matches!(self.selected_tab, EditorTab::Map | EditorTab::Textures) {
            return root;
        }
        macro_rules! edit {
            ($root:expr,$action:ty,$variant:ident) => {
                $root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    if this.dialogs.read(cx).active_popup().is_some() {
                        cx.stop_propagation();
                        return;
                    }
                    if this.text_input_focused(window, cx) {
                        cx.propagate();
                        return;
                    }
                    if this.selected_tab == EditorTab::Textures {
                        this.textures.update(cx, |tab, cx| {
                            tab.edit_action(TextureEditAction::$variant, window, cx)
                        });
                    } else {
                        this.map
                            .update(cx, |map, cx| map.command(MapCommand::$variant, cx));
                    }
                    cx.notify();
                    cx.stop_propagation();
                }))
            };
        }
        let root = edit!(root, ui_common::Copy, Copy);
        let root = edit!(root, ui_common::Cut, Cut);
        let root = edit!(root, ui_common::Paste, Paste);
        let root = edit!(root, ui_common::Undo, Undo);
        edit!(root, ui_common::Redo, Redo)
    }
}
