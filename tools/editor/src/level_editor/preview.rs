//! Hover wall preview: wall-elevation popup when pointing at a line.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use editor_core::geom::line_length;
use editor_core::{EditorMap, LineDef, Sector, SideDef};
use slint::{ComponentHandle as _, Timer};

use crate::SharedState;
use crate::assets::EditorAssets;
use crate::boundary::{SelectMode, TexSlot, Tool};
use crate::generated::{CanvasController, EditorWindow, SectorPreview, WallPreview, WallSide};
use crate::gfx::{WallBand, render_flat_square, render_wall};
use crate::level_editor::pick3d::PickKind;
use crate::prefs::PreviewMode;
use crate::state::{DragState, SectorFill};

/// Popup offset from the cursor.
const CURSOR_OFFSET: f32 = 18.0;

thread_local! {
    static HOVER_TIMER: Timer = Timer::default();
}

/// Wall bands for one side, cropped to the textured range top to bottom; `None` if none textured.
fn side_bands(
    map: &EditorMap,
    line: &LineDef,
    side: &SideDef,
    is_front: bool,
) -> Option<Vec<WallBand>> {
    let s = map.sectors.get(side.sector?)?;
    let other = if is_front {
        line.back.as_ref()
    } else {
        Some(&line.front)
    };
    let other = other
        .and_then(|o| o.sector)
        .and_then(|s| map.sectors.get(s));

    let mut raw: Vec<WallBand> = side_bands_tagged(s, other, side, is_front)
        .into_iter()
        .map(|(_, band)| band)
        .collect();
    let first = raw.iter().position(|b| b.tex.is_some())?;
    let last = raw.iter().rposition(|b| b.tex.is_some())?;
    let kept: Vec<WallBand> = raw.drain(first..=last).collect();
    Some(kept)
}

/// Like `side_bands` but tagged with `TexSlot` and uncropped.
pub(crate) fn side_bands_tagged(
    s: &Sector,
    other: Option<&Sector>,
    side: &SideDef,
    is_front: bool,
) -> Vec<(TexSlot, WallBand)> {
    let (top, mid, bottom) = if is_front {
        (TexSlot::FrontTop, TexSlot::FrontMid, TexSlot::FrontBottom)
    } else {
        (TexSlot::BackTop, TexSlot::BackMid, TexSlot::BackBottom)
    };

    let mut bands = Vec::with_capacity(3);
    match other {
        None => {
            let h = (s.ceil_height - s.floor_height) as f32;
            if h > 0.0 {
                bands.push((
                    mid,
                    WallBand {
                        tex: (!side.middle_tex.is_empty()).then_some(side.middle_tex),
                        height: h,
                        masked: false,
                    },
                ));
            }
        }
        Some(o) => {
            let upper = (s.ceil_height - o.ceil_height) as f32;
            if upper > 0.0 {
                bands.push((
                    top,
                    WallBand {
                        tex: (!side.top_tex.is_empty()).then_some(side.top_tex),
                        height: upper,
                        masked: false,
                    },
                ));
            }
            let middle =
                (s.ceil_height.min(o.ceil_height) - s.floor_height.max(o.floor_height)) as f32;
            if middle > 0.0 {
                bands.push((
                    mid,
                    WallBand {
                        tex: (!side.middle_tex.is_empty()).then_some(side.middle_tex),
                        height: middle,
                        masked: true,
                    },
                ));
            }
            let lower = (o.floor_height - s.floor_height) as f32;
            if lower > 0.0 {
                bands.push((
                    bottom,
                    WallBand {
                        tex: (!side.bottom_tex.is_empty()).then_some(side.bottom_tex),
                        height: lower,
                        masked: false,
                    },
                ));
            }
        }
    }
    bands
}

struct SidePreview {
    image: slint::Image,
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
    let image = render_wall(assets, bands, width_world, px_per_unit * pixel_ratio)?;
    Some(SidePreview {
        image,
        width: width_world * px_per_unit,
        height: total * px_per_unit,
    })
}

/// False → hides and clears hover state.
fn hoverable(state: &mut SharedState, canvas: &CanvasController) -> bool {
    let ok = matches!(state.app.tool, Tool::Select(_) | Tool::Sector)
        && state.app.drag == DragState::None
        && state.app.map.is_some();
    if !ok && state.map_render.hovered_line.take().is_some() {
        hide_wall_preview(canvas);
    }
    ok
}

/// Pick line/sector under `pos`; sector suppressed in Texture fill mode. Returns true if changed.
fn probe_target(state: &mut SharedState, pos: [f32; 2]) -> bool {
    let (line, sector) = match state
        .app
        .pick_3d_select(pos, SelectMode::All)
        .map(|h| h.kind)
    {
        Some(PickKind::Linedef(i)) => (Some(i), None),
        Some(PickKind::Sector(s)) if state.app.sector_fill != SectorFill::Texture => {
            (None, Some(s))
        }
        _ => (None, None),
    };
    let changed =
        line != state.map_render.hovered_line || sector != state.map_render.hovered_sector;
    state.map_render.hovered_line = line;
    state.map_render.hovered_sector = sector;
    changed
}

fn hide_wall_preview(canvas: &CanvasController) {
    canvas.set_wall_preview(WallPreview {
        pinned: canvas.get_wall_preview().pinned,
        ..WallPreview::default()
    });
}

fn follow_cursor(canvas: &CanvasController, pos: [f32; 2]) {
    canvas.set_wall_preview(WallPreview {
        x: pos[0] + CURSOR_OFFSET,
        y: pos[1] + CURSOR_OFFSET,
        ..canvas.get_wall_preview()
    });
}

/// Push preview mode to canvas and clear any open card.
pub fn push_preview_mode(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let canvas = ui.global::<CanvasController>();
    let mode = shared.borrow().prefs.preview_mode;
    canvas.set_wall_preview(WallPreview {
        pinned: mode == PreviewMode::PinnedCorner,
        ..WallPreview::default()
    });
    HOVER_TIMER.with(Timer::stop);
    let state = &mut *shared.borrow_mut();
    state.map_render.hovered_line = None;
    state.map_render.hovered_sector = None;
}

/// Update preview after pointer move.
pub fn handle_hover(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, pos: [f32; 2]) {
    let mode = shared.borrow().prefs.preview_mode;
    match mode {
        PreviewMode::HoverDelayed => hover_delayed(ui, shared, pos),
        PreviewMode::PinnedCorner => pinned_corner(ui, shared, pos),
        PreviewMode::OnClick => {}
    }
}

fn hover_delayed(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, pos: [f32; 2]) {
    let canvas = ui.global::<CanvasController>();
    let can_hover = hoverable(&mut shared.borrow_mut(), &canvas);
    if !can_hover {
        HOVER_TIMER.with(Timer::stop);
        return;
    }
    let changed = probe_target(&mut shared.borrow_mut(), pos);
    follow_cursor(&canvas, pos);
    if !changed {
        return;
    }
    hide_wall_preview(&canvas);
    let delay = shared.borrow().prefs.preview_hover_delay_ms.max(0.0);
    let weak = ui.as_weak();
    let s = shared.clone();

    HOVER_TIMER.with(|t| {
        t.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(delay as u64),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                let pixel_ratio = ui.window().scale_factor();
                rebuild(&ui, &mut s.borrow_mut(), pixel_ratio);
            },
        );
    });
}

fn pinned_corner(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, pos: [f32; 2]) {
    let canvas = ui.global::<CanvasController>();
    let pixel_ratio = ui.window().scale_factor();
    let state = &mut *shared.borrow_mut();
    if !hoverable(state, &canvas) {
        return;
    }
    if probe_target(state, pos) {
        rebuild(ui, state, pixel_ratio);
    }
}

pub fn handle_pick(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, pos: [f32; 2]) {
    if shared.borrow().prefs.preview_mode != PreviewMode::OnClick {
        return;
    }
    let canvas = ui.global::<CanvasController>();
    let pixel_ratio = ui.window().scale_factor();
    {
        let state = &mut *shared.borrow_mut();
        if !hoverable(state, &canvas) {
            return;
        }
        probe_target(state, pos);
        rebuild(ui, state, pixel_ratio);
    }
    follow_cursor(&canvas, pos);
}

fn rebuild(ui: &EditorWindow, state: &mut SharedState, pixel_ratio: f32) {
    let canvas = ui.global::<CanvasController>();
    let Some(key) = state.map_render.hovered_line else {
        rebuild_sector(ui, state, pixel_ratio);
        return;
    };
    let Some(map) = &state.app.map else {
        hide_wall_preview(&canvas);
        return;
    };
    let Some(line) = map.lines.get(key) else {
        hide_wall_preview(&canvas);
        return;
    };
    // Zero-length: bail before width divisions produce inf/NaN.
    let width_world = match line_length(map, line) {
        Some(w) if w > 0.0 => w,
        _ => {
            hide_wall_preview(&canvas);
            return;
        }
    };

    let front_bands = side_bands(map, line, &line.front, true);
    let back_bands = line
        .back
        .as_ref()
        .and_then(|b| side_bands(map, line, b, false));
    if front_bands.is_none() && back_bands.is_none() {
        hide_wall_preview(&canvas);
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
        hide_wall_preview(&canvas);
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
        hide_wall_preview(&canvas);
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

    canvas.set_sector_preview(SectorPreview::default());
    let prev = canvas.get_wall_preview();
    canvas.set_wall_preview(WallPreview {
        visible: front.is_some() || back.is_some(),
        front: wall_side(front.as_ref()),
        back: wall_side(back.as_ref()),
        ..prev
    });
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

fn rebuild_sector(ui: &EditorWindow, state: &mut SharedState, pixel_ratio: f32) {
    let canvas = ui.global::<CanvasController>();
    let Some(sector) = state
        .map_render
        .hovered_sector
        .and_then(|k| state.app.map.as_ref()?.sectors.get(k))
        .copied()
    else {
        hide_wall_preview(&canvas);
        return;
    };

    let side = state
        .prefs
        .wall_preview_max_w
        .min(state.prefs.wall_preview_max_h / 2.0)
        .max(32.0);
    if !state.ensure_assets() {
        hide_wall_preview(&canvas);
        return;
    }
    let assets = state.assets.as_ref().expect("ensured above");
    let physical = (side * pixel_ratio) as u32;
    let ceil = render_flat_square(assets, sector.ceil_flat, physical);
    let floor = render_flat_square(assets, sector.floor_flat, physical);

    canvas.set_sector_preview(SectorPreview {
        visible: true,
        ceil_img: ceil,
        floor_img: floor,
        side,
    });
    let prev = canvas.get_wall_preview();
    canvas.set_wall_preview(WallPreview {
        visible: true,
        front: WallSide::default(),
        back: WallSide::default(),
        ..prev
    });
}
