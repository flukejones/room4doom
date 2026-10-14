//! Project resource, node and launch settings.

use super::{DialogsView, PathField};
use crate::app::preferences as prefs;
use crate::app::session::EditorSession;
use crate::defaults::launch_type_choices;
use editor_core::NodesFormat;
use gpui::{AnyElement, Context, Window, div, prelude::*};
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct ProjectSettingsDraft {
    pub(crate) active: bool,
    pub(crate) iwad_path: String,
    pub(crate) nodes_format: NodesFormat,
    pub(crate) launch_type: i32,
    pub(crate) launch_choices: std::rc::Rc<[(i32, gpui::SharedString)]>,
    pub(crate) last_map: String,
}

impl Default for ProjectSettingsDraft {
    fn default() -> Self {
        Self {
            active: false,
            iwad_path: Default::default(),
            nodes_format: NodesFormat::Room4Doom,
            launch_type: 1,
            launch_choices: Default::default(),
            last_map: Default::default(),
        }
    }
}

impl DialogsView {
    pub(super) fn project_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = &self.drafts.project_settings;
        if !draft.active {
            return self
                .forms
                .panel(cx)
                .child("Open a project to edit its settings.")
                .into_any_element();
        }
        let path = self.forms.field(
            "project-iwad",
            "IWAD",
            &draft.iwad_path,
            cx.listener(|this, value: &String, _, cx| {
                this.drafts.project_settings.iwad_path = value.clone();
                cx.notify();
            }),
            |_, _, _| {},
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(path)
            .child(self.forms.button(
                "project-iwad-browse",
                "Browse IWAD…",
                cx.listener(|this, _, _, cx| this.pick_path(PathField::ProjectIwad, cx)),
            ))
            .child(self.forms.typed_select_field(
                "project-nodes",
                "Nodes",
                [
                    (NodesFormat::Room4Doom, "Room4Doom"),
                    (NodesFormat::Classic, "Classic"),
                    (NodesFormat::Both, "Both"),
                ],
                Some(&draft.nodes_format),
                cx.listener(|this, format: &NodesFormat, _, cx| {
                    this.drafts.project_settings.nodes_format = *format;
                    cx.notify();
                }),
                window,
                cx,
            ))
            .child(self.forms.typed_select_field(
                "project-launch-type",
                "Launch type",
                draft.launch_choices.clone(),
                Some(&draft.launch_type),
                cx.listener(|this, kind: &i32, _, cx| {
                    this.drafts.project_settings.launch_type = *kind;
                    cx.notify();
                }),
                window,
                cx,
            ))
            .child(format!("Last map: {}", draft.last_map))
            .into_any_element()
    }
}

/// Open edge: fill from open project; disabled when none.
pub(crate) fn populate(draft: &mut ProjectSettingsDraft, state: &EditorSession) {
    let Some(project) = &state.project else {
        draft.active = false;
        return;
    };
    let settings = &project.settings;
    draft.active = true;
    draft.iwad_path = settings.iwad.display().to_string();
    draft.nodes_format = settings.nodes_format;
    draft.launch_type = settings.launch_type;
    draft.launch_choices = launch_type_choices(settings.launch_type);
    draft.last_map = settings.last_map.clone().unwrap_or_default();
}

/// Apply edge: write fields to project, persist.
pub(crate) fn apply(draft: &ProjectSettingsDraft, state: &mut EditorSession) {
    let iwad = PathBuf::from(draft.iwad_path.clone());

    let iwad_changed = state.iwad.as_deref() != Some(iwad.as_path());
    let Some(project) = state.project.as_mut() else {
        return;
    };
    project.settings.iwad = iwad.clone();
    project.settings.nodes_format = draft.nodes_format;
    project.settings.launch_type = draft.launch_type;
    if let Err(e) = project.save() {
        log::error!("save project settings: {e}");
    }
    if iwad_changed {
        // invalidates gfx/asset caches
        state.set_iwad(iwad);
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("save project-settings prefs: {e}");
    }
}
