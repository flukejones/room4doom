//! GPUI map editor with incremental GPU rendering and background export jobs.

mod app;
mod assets;
mod boundary;
mod bsp_anim;
mod components;
mod defaults;
mod dialogs;
mod gfx;
mod launch;
mod level_editor;
mod light_anim;
#[cfg(test)]
mod performance;
mod png_export;
mod render;
mod tabs;
mod theme;
mod thing_info;
mod undo;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use argh::FromArgs;
use gpui::AppContext as _;

use crate::app::project::{OpenWad, open_from_options, open_project};
use crate::app::session::EditorSession;
use crate::app::{commands, preferences as prefs};
use crate::dialogs::DialogRequest;
use crate::level_editor::LevelEditorState;

use crate::app::appearance::EditorAppearance;
use crate::app::window::EditorShell;

/// ReDoomEd-style Doom map editor.
#[derive(FromArgs)]
struct Options {
    /// IWAD to open maps from
    #[argh(option)]
    iwad: Option<PathBuf>,
    /// PWAD to load alongside the IWAD (repeatable); appended after the IWAD so its lumps override (engine load order)
    #[argh(option)]
    pwad: Vec<PathBuf>,
    /// map name to open (e.g. E1M1); with --iwad
    #[argh(option)]
    map: Option<String>,
    /// open a DoomEd project (.dpr) for things/specials/textures
    #[argh(option)]
    project: Option<PathBuf>,
}

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("logger initializes once");

    let options: Options = argh::from_env();
    let user_prefs = prefs::load_prefs();
    let mut app = LevelEditorState::new();
    app.grid = user_prefs.grid.max(1);
    app.snap = user_prefs.snap;
    app.snap_to_vertex = user_prefs.snap_to_vertex;
    app.snap_to_line = user_prefs.snap_to_line;
    app.angle_snap = user_prefs.angle_snap;
    let dark = prefs::resolve_dark(&user_prefs, false);
    let (name, variant) = if dark {
        (user_prefs.dark_theme.as_str(), opaline::ThemeVariant::Dark)
    } else {
        (
            user_prefs.light_theme.as_str(),
            opaline::ThemeVariant::Light,
        )
    };
    app.style = theme::resolve(name, variant).1;
    let iwad = options.iwad.clone();
    let shared = Rc::new(RefCell::new(EditorSession::new(app, iwad, user_prefs)));

    shared.borrow_mut().wgpu = render::wgpu::initialize().expect("GPU canvas initializes");
    let appearance = EditorAppearance::new(&shared);

    if let Some(project) = &options.project {
        open_project(&shared, project);
    }
    let startup = open_from_options(&shared, &options);

    ui_common::application().run(move |cx| {
        let quit_shared = shared.clone();
        ui_common::init_with_quit("Room4Doom Editor", cx, move |cx| {
            if let Some(window) = cx
                .active_window()
                .and_then(|window| window.downcast::<EditorShell>())
            {
                let _ = window.update(cx, |view, window, cx| view.request_quit(window, cx));
            } else {
                let shared = quit_shared.clone();
                cx.spawn(async move |cx| {
                    if commands::run(&shared, commands::AppCommand::Quit).quit {
                        cx.update(|cx| cx.quit());
                    }
                })
                .detach();
            }
        });
        let options = ui_common::window_options(
            "Room4Doom Editor".into(),
            gpui::size(gpui::px(1280.0), gpui::px(850.0)),
            cx,
        );
        cx.open_window(options, move |window, cx| {
            cx.new(|cx| {
                let mut view = EditorShell::new(appearance, shared, window, cx);
                if matches!(startup, OpenWad::ChooseMap) {
                    view.queue_dialog(DialogRequest::ChooseMap);
                }
                view.initialize_native(window, cx);
                view
            })
        })
        .expect("editor window opens");
        cx.activate(true);
    });
}
