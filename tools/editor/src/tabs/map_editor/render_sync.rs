//! Incremental scene reconciliation and presentation for a map-local state bundle.
use super::presentation::MapPresentation;
use crate::app::preferences::BspAnimPref;
use crate::app::session::EditorSession;
use crate::bsp_anim;
use crate::level_editor::state::Damage;
use crate::render::sync;
use std::time::{Duration, Instant};
const LIGHT_TIC: Duration = Duration::from_millis(1000 / 35);
const CAMERA_TIC: Duration = Duration::from_millis(16);

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "logical viewport dimensions become physical texture dimensions"
)]
fn metrics(ui: &MapPresentation, state: &EditorSession) -> ([u32; 2], f32) {
    let scale = ui.pixel_ratio.max(1.);
    let [width, height] = state.map_editor.camera.viewport();
    let size = [
        (width * scale).round().max(1.) as u32,
        (height * scale).round().max(1.) as u32,
    ];
    let ratio = if width > 0. {
        size[0] as f32 / width
    } else {
        1.
    };
    (size, ratio)
}
pub(crate) fn paint(ui: &mut MapPresentation, state: &EditorSession) {
    let (size, _) = metrics(ui, state);
    if let Some(image) = sync::paint(state, size) {
        ui.canvas.wgpu_img = image;
    }
}
pub(crate) fn play_build(
    ui: &mut MapPresentation,
    state: &mut EditorSession,
    events: Vec<rbsp::BuildEvent>,
    interval_ms: u64,
    keep_all: bool,
) {
    let mode = state.prefs.bsp_anim;
    if mode == BspAnimPref::Off || events.is_empty() {
        return;
    }
    ui.light_deadline = None;
    ui.bsp_deadline = None;
    bsp_anim::start(state, ui.pixel_ratio, events, mode, keep_all);
    ui.bsp_interval = Duration::from_millis(
        interval_ms.clamp(bsp_anim::MIN_INTERVAL_MS, bsp_anim::MAX_INTERVAL_MS),
    );
    if mode == BspAnimPref::Timed && state.bsp_anim.is_some() {
        ui.bsp_deadline = Some(Instant::now() + ui.bsp_interval);
    }
    paint(ui, state);
}
pub(crate) fn apply_damage(ui: &mut MapPresentation, state: &mut EditorSession, damage: Damage) {
    if damage == Damage::None {
        return;
    }
    bsp_anim::clear(state);
    ui.bsp_deadline = None;
    let (_, ratio) = metrics(ui, state);
    if matches!(damage, Damage::Edited | Damage::Restyle) {
        ui.panels_key = None;
    }
    let lights_changed = sync::apply_damage(state, damage, ratio);
    if damage != Damage::Overlay {
        if sync::refresh_light_anim(state, lights_changed) {
            ui.light_deadline
                .get_or_insert_with(|| Instant::now() + LIGHT_TIC);
        } else {
            ui.light_deadline = None;
        }
        if state.map_editor.camera.needs_ease() {
            ui.camera_deadline = Some(Instant::now() + CAMERA_TIC);
        }
    }
    paint(ui, state);
}
pub(crate) fn tick(ui: &mut MapPresentation, state: &mut EditorSession, now: Instant) -> bool {
    let hover_changed = super::preview::tick(ui, state, now);
    let camera_changed = if ui.camera_deadline.is_some_and(|deadline| now >= deadline) {
        let more = state.map_editor.ease_camera();
        ui.camera_deadline = more.then_some(now + CAMERA_TIC);
        let (_, ratio) = metrics(ui, state);
        sync::regrid(state, ratio);
        true
    } else {
        false
    };
    let light_changed = if ui.light_deadline.is_some_and(|deadline| now >= deadline) {
        let more = sync::light_tic(state);
        ui.light_deadline = more.then_some(now + LIGHT_TIC);
        true
    } else {
        false
    };
    let bsp_changed = if ui.bsp_deadline.is_some_and(|deadline| now >= deadline) {
        let finished = bsp_anim::advance(state);
        ui.bsp_deadline = (!finished).then_some(now + ui.bsp_interval);
        true
    } else {
        false
    };
    let canvas_changed = camera_changed || light_changed || bsp_changed;
    if canvas_changed {
        paint(ui, state);
    }
    hover_changed || canvas_changed
}
