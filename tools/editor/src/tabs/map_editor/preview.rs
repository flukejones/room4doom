//! Hover and click preview preparation, owned by the map tab.
use super::canvas::CanvasData;
use super::presentation::MapPresentation;
use crate::app::preferences::PreviewMode;
use crate::app::session::EditorSession;
use crate::assets::EditorAssets;
use crate::boundary::{SelectMode, Tool};
use crate::components::types::{SectorPreview, WallPreview, WallSide};
use crate::gfx::image::UiImage;
use crate::gfx::{WallBand, render_flat_square, render_wall};
use crate::level_editor::pick3d::PickKind;
use crate::level_editor::preview::side_bands;
use crate::level_editor::state::{DragState, SectorFill};
use editor_core::geom::line_length;
use std::time::{Duration, Instant};

const CURSOR_OFFSET: f32 = 18.;
struct SidePreview {
    image: UiImage,
    width: f32,
    height: f32,
}
fn render_side(
    assets: &EditorAssets,
    bands: &[WallBand],
    width_world: f32,
    px_per_unit: f32,
    pixel_ratio: f32,
) -> Option<SidePreview> {
    let total: f32 = bands.iter().map(|b| b.height).sum();
    Some(SidePreview {
        image: render_wall(assets, bands, width_world, px_per_unit * pixel_ratio)?,
        width: width_world * px_per_unit,
        height: total * px_per_unit,
    })
}
fn hoverable(ui: &mut MapPresentation, state: &EditorSession) -> bool {
    let ok = matches!(state.map_editor.tool, Tool::Select(_) | Tool::Sector)
        && state.map_editor.drag == DragState::None
        && state.map_editor.map.is_some();
    if !ok {
        ui.hovered_line = None;
        ui.hovered_sector = None;
        hide_wall_preview(&mut ui.canvas);
        ui.hover_deadline = None;
    }
    ok
}
fn probe_target(ui: &mut MapPresentation, state: &EditorSession, pos: [f32; 2]) -> bool {
    let (line, sector) = match state
        .map_editor
        .pick_3d_select(pos, SelectMode::All)
        .map(|h| h.kind)
    {
        Some(PickKind::Linedef(i)) => (Some(i), None),
        Some(PickKind::Sector(s)) if state.map_editor.sector_fill != SectorFill::Texture => {
            (None, Some(s))
        }
        _ => (None, None),
    };
    let changed = line != ui.hovered_line || sector != ui.hovered_sector;
    ui.hovered_line = line;
    ui.hovered_sector = sector;
    changed
}
fn hide_wall_preview(canvas: &mut CanvasData) {
    canvas.wall_preview = WallPreview {
        pinned: canvas.wall_preview.pinned,
        ..WallPreview::default()
    };
    canvas.sector_preview = SectorPreview::default();
}
fn follow_cursor(canvas: &mut CanvasData, pos: [f32; 2]) {
    canvas.wall_preview.x = pos[0] + CURSOR_OFFSET;
    canvas.wall_preview.y = pos[1] + CURSOR_OFFSET;
}
pub(crate) fn reset(ui: &mut MapPresentation, state: &EditorSession) {
    ui.canvas.wall_preview = WallPreview {
        pinned: state.prefs.preview_mode == PreviewMode::PinnedCorner,
        ..WallPreview::default()
    };
    ui.canvas.sector_preview = SectorPreview::default();
    ui.hovered_line = None;
    ui.hovered_sector = None;
    ui.hover_deadline = None;
}
pub(crate) fn handle_hover(ui: &mut MapPresentation, state: &mut EditorSession, pos: [f32; 2]) {
    if !hoverable(ui, state) {
        return;
    }
    match state.prefs.preview_mode {
        PreviewMode::OnClick => {}
        PreviewMode::PinnedCorner => {
            if probe_target(ui, state, pos) {
                rebuild(ui, state, ui.pixel_ratio);
            }
        }
        PreviewMode::HoverDelayed => {
            let changed = probe_target(ui, state, pos);
            follow_cursor(&mut ui.canvas, pos);
            if changed {
                hide_wall_preview(&mut ui.canvas);
                ui.hover_deadline = Some(
                    Instant::now()
                        + Duration::from_secs_f64(
                            f64::from(state.prefs.preview_hover_delay_ms.max(0.)) / 1000.,
                        ),
                );
            }
        }
    }
}
pub(crate) fn handle_pick(ui: &mut MapPresentation, state: &mut EditorSession, pos: [f32; 2]) {
    if state.prefs.preview_mode != PreviewMode::OnClick || !hoverable(ui, state) {
        return;
    }
    probe_target(ui, state, pos);
    rebuild(ui, state, ui.pixel_ratio);
    follow_cursor(&mut ui.canvas, pos);
}
pub(crate) fn tick(ui: &mut MapPresentation, state: &mut EditorSession, now: Instant) -> bool {
    if ui.hover_deadline.is_none_or(|deadline| now < deadline) {
        return false;
    }
    ui.hover_deadline = None;
    rebuild(ui, state, ui.pixel_ratio);
    true
}
fn rebuild(ui: &mut MapPresentation, state: &mut EditorSession, pixel_ratio: f32) {
    let canvas = &mut ui.canvas;
    let Some(key) = ui.hovered_line else {
        rebuild_sector(ui, state, pixel_ratio);
        return;
    };
    let Some(map) = &state.map_editor.map else {
        hide_wall_preview(canvas);
        return;
    };
    let Some(line) = map.lines.get(key) else {
        hide_wall_preview(canvas);
        return;
    };
    // Zero-length: bail before width divisions produce inf/NaN.
    let width_world = match line_length(map, line) {
        Some(w) if w > 0.0 => w,
        _ => {
            hide_wall_preview(canvas);
            return;
        }
    };

    let front_bands = side_bands(map, line, &line.front, true);
    let back_bands = line
        .back
        .as_ref()
        .and_then(|b| side_bands(map, line, b, false));
    if front_bands.is_none() && back_bands.is_none() {
        hide_wall_preview(canvas);
        return;
    }

    let both_sides = front_bands.is_some() && back_bands.is_some();
    let width_factor = if both_sides { 0.5 } else { 1.0 };
    let (min_w, min_h, max_w, max_h) = (
        state.prefs.wall_preview_min_w * width_factor,
        state.prefs.wall_preview_min_h,
        state.prefs.wall_preview_max_w * width_factor,
        state.prefs.wall_preview_max_h,
    );
    if !state.ensure_assets() {
        hide_wall_preview(canvas);
        return;
    }
    let assets = state.assets.as_ref().expect("ensured above");

    // Shared scale: fit tallest band-stack + line length into max box; floor at min.
    let tallest = [&front_bands, &back_bands]
        .into_iter()
        .flatten()
        .map(|bands| bands.iter().map(|b| b.height).sum::<f32>())
        .fold(0.0f32, f32::max);
    if tallest <= 0.0 {
        hide_wall_preview(canvas);
        return;
    }
    let fit = (max_w / width_world).min(max_h / tallest);
    let floor = (min_w / width_world).min(min_h / tallest);
    let px_per_unit = fit.max(floor);

    let front = front_bands
        .as_deref()
        .and_then(|b| render_side(assets, b, width_world, px_per_unit, pixel_ratio));
    let back = back_bands
        .as_deref()
        .and_then(|b| render_side(assets, b, width_world, px_per_unit, pixel_ratio));

    canvas.sector_preview = SectorPreview::default();
    let prev = canvas.wall_preview.clone();
    canvas.wall_preview = WallPreview {
        visible: front.is_some() || back.is_some(),
        front: wall_side(front.as_ref()),
        back: wall_side(back.as_ref()),
        ..prev
    };
}

fn wall_side(side: Option<&SidePreview>) -> WallSide {
    match side {
        Some(s) => WallSide {
            visible: true,
            img: s.image.clone(),
            w: s.width,
            h: s.height,
        },
        None => WallSide::default(),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
fn rebuild_sector(ui: &mut MapPresentation, state: &mut EditorSession, pixel_ratio: f32) {
    let canvas = &mut ui.canvas;
    let Some(sector) = ui
        .hovered_sector
        .and_then(|k| state.map_editor.map.as_ref()?.sectors.get(k))
        .copied()
    else {
        hide_wall_preview(canvas);
        return;
    };

    let side = state
        .prefs
        .wall_preview_max_w
        .min(state.prefs.wall_preview_max_h / 2.0)
        .max(32.0);
    if !state.ensure_assets() {
        hide_wall_preview(canvas);
        return;
    }
    let assets = state.assets.as_ref().expect("ensured above");
    let physical = (side * pixel_ratio) as u32;
    let ceil = render_flat_square(assets, sector.ceil_flat, physical);
    let floor = render_flat_square(assets, sector.floor_flat, physical);

    canvas.sector_preview = SectorPreview {
        visible: true,
        ceil_img: ceil,
        floor_img: floor,
        side,
    };
    let prev = canvas.wall_preview.clone();
    canvas.wall_preview = WallPreview {
        visible: true,
        front: WallSide::default(),
        back: WallSide::default(),
        ..prev
    };
}
