//! BSP build animation overlay: replays recorded [`rbsp::BuildEvent`]s as world-space GPU instances on the shared overlay layer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use rbsp::BuildEvent;
use slint::{ComponentHandle as _, Timer};

use crate::SharedState;
use crate::generated::EditorWindow;
use crate::prefs::BspAnimPref;
use crate::render::frame::{push_line, push_marker, rgba_f32};
use crate::render::repaint_canvas;
use crate::render::stop_light_timer;
use crate::render::style::Color;
use crate::render::wgpu::{LineInst, MarkerInst};

/// User-tunable partition delay range (ms).
pub const MIN_INTERVAL_MS: u64 = 1;
pub const MAX_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_INTERVAL_MS: u64 = 100;
const ANIM_SEG_LEFT: Color = [0x20, 0xb0, 0x30, 0xff];
const ANIM_SEG_RIGHT: Color = [0xe0, 0x70, 0x10, 0xff];
const ANIM_AABB_LEFT: Color = [0xd0, 0x20, 0xc0, 0xff];
const ANIM_AABB_RIGHT: Color = [0xe0, 0x20, 0x20, 0xff];
const ANIM_SUBSECTOR: Color = [0x10, 0xa0, 0xd0, 0xc0];
const ANIM_SEG_VERTEX_PX: f32 = 5.0;
const ANIM_AABB_HALO_THICKNESS: f32 = 4.0;
const ANIM_AABB_CORE_THICKNESS: f32 = 2.0;
/// Outward world-unit pad so the AABB clears its bounded geometry.
const ANIM_AABB_PAD_WORLD: f32 = 4.0;
const ANIM_PARTITION_THICKNESS: f32 = 3.0;
const ANIM_SEG_THICKNESS: f32 = 3.0;
const ANIM_SUBSECTOR_THICKNESS: f32 = 1.0;

thread_local! {
    // thread_local (not SharedState) so the timer arms without borrowing `shared`.
    static ANIM_TIMER: Timer = Timer::default();
}

pub struct BspAnim {
    events: Vec<BuildEvent>,
    next: usize,
    instant: bool,
    /// Accumulate all partitions or only the current step.
    keep_all: bool,
    pixel_ratio: f32,
    /// Half-diagonal of map bounds; extends divlines across the map.
    world_span: f32,
    /// Persisted geometry (current step's segs are transient, not stored here).
    divlines: Vec<([f32; 2], [f32; 2])>,
    subsectors: Vec<Vec<[f32; 2]>>,
    seg_verts: Vec<([f32; 2], Color)>,
    divline: Color,
    aabb_halo: Color,
}

/// Begin (or replace) build animation replay.
pub fn start(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    events: Vec<BuildEvent>,
    mode: BspAnimPref,
    interval_ms: u64,
    keep_all: bool,
) {
    if mode == BspAnimPref::Off || events.is_empty() {
        return;
    }
    let pixel_ratio = ui.window().scale_factor();
    let world_span = events_world_span(&events);
    let instant = matches!(mode, BspAnimPref::Instant);
    let partition_count = events
        .iter()
        .filter(|e| matches!(e, BuildEvent::PartitionChosen { .. }))
        .count();
    let subsector_count = events
        .iter()
        .filter(|e| matches!(e, BuildEvent::SubsectorDone { .. }))
        .count();
    let seg_vert_count: usize = events
        .iter()
        .map(|e| match e {
            BuildEvent::SegsSplit {
                left,
                right,
                ..
            } => 2 * (left.len() + right.len()),
            _ => 0,
        })
        .sum();

    stop_light_timer();
    {
        let state = &mut *shared.borrow_mut();
        let style = &state.app.style;
        let aabb_halo = if luminance(style.back) < 0.5 {
            [0x00, 0x00, 0x00, 0xd0]
        } else {
            [0xff, 0xff, 0xff, 0xd0]
        };
        let divline = with_alpha(style.two_sided, 0xb0);
        state.bsp_anim = Some(BspAnim {
            events,
            next: 0,
            instant,
            keep_all,
            pixel_ratio,
            world_span,
            divlines: Vec::with_capacity(partition_count),
            subsectors: Vec::with_capacity(subsector_count),
            seg_verts: Vec::with_capacity(seg_vert_count),
            divline,
            aabb_halo,
        });
    }

    let _ = advance(ui, shared);
    if instant {
        return;
    }

    let interval = Duration::from_millis(interval_ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS));
    let weak = ui.as_weak();
    let s = shared.clone();
    ANIM_TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, interval, move || {
            let Some(ui) = weak.upgrade() else { return };
            if advance(&ui, &s) {
                ANIM_TIMER.with(Timer::stop);
            }
        });
    });
}

/// Advance overlay by one partition step. Returns `true` when finished.
fn advance(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) -> bool {
    let finished = {
        let state = &mut *shared.borrow_mut();
        let SharedState {
            bsp_anim,
            wgpu,
            ..
        } = state;
        let Some(anim) = bsp_anim.as_mut() else {
            return true;
        };
        let end = if anim.instant {
            anim.events.len()
        } else {
            partition_step_end(&anim.events, anim.next)
        };
        if !anim.keep_all {
            anim.divlines.clear();
            anim.subsectors.clear();
            anim.seg_verts.clear();
        }
        let step = &anim.events[anim.next..end];
        let step_seg_count: usize = step
            .iter()
            .map(|e| match e {
                BuildEvent::SegsSplit {
                    left,
                    right,
                    ..
                } => left.len() + right.len(),
                _ => 0,
            })
            .sum();
        let mut step_segs: Vec<([f32; 2], [f32; 2], Color)> = Vec::with_capacity(step_seg_count);
        let mut step_boxes: Vec<([f32; 2], [f32; 2], Color)> = Vec::new();
        for ev in step {
            match ev {
                BuildEvent::PartitionChosen {
                    p1,
                    p2,
                } => anim.divlines.push((*p1, *p2)),
                BuildEvent::SegsSplit {
                    left,
                    right,
                    left_bbox,
                    right_bbox,
                } => {
                    step_segs.clear();
                    step_boxes.clear();
                    let sides = [
                        (left, ANIM_SEG_LEFT, ANIM_AABB_LEFT, left_bbox),
                        (right, ANIM_SEG_RIGHT, ANIM_AABB_RIGHT, right_bbox),
                    ];
                    for (segs, base, box_colour, bbox) in sides {
                        for (i, seg) in segs.iter().enumerate() {
                            let colour = shade(base, i);
                            step_segs.push((seg[0], seg[1], colour));
                            anim.seg_verts.push((seg[0], colour));
                            anim.seg_verts.push((seg[1], colour));
                        }
                        if let Some(bbox) = bbox {
                            step_boxes.push((bbox[0], bbox[1], box_colour));
                        }
                    }
                }
                BuildEvent::SubsectorDone {
                    verts,
                } => anim.subsectors.push(verts.clone()),
            }
        }
        anim.next = end;

        let mut lines = Vec::new();
        let mut markers = Vec::new();
        draw_kept(anim, &mut lines, &mut markers);
        draw_step_segs(anim, &mut lines, &step_segs, &step_boxes);
        wgpu.set_overlay(&lines, &markers);
        anim.next >= anim.events.len()
    };

    repaint_canvas(ui, shared);
    finished
}

/// Emit persisted divlines, subsector outlines, seg-vertex dots.
fn draw_kept(anim: &BspAnim, lines: &mut Vec<LineInst>, markers: &mut Vec<MarkerInst>) {
    let pr = anim.pixel_ratio;
    let divline = rgba_f32(anim.divline);
    for &(p1, p2) in &anim.divlines {
        let (ea, eb) = extend_to_bounds(p1, p2, anim.world_span);
        push_line(lines, ea, eb, ANIM_PARTITION_THICKNESS * pr, divline);
    }
    let subsector = rgba_f32(ANIM_SUBSECTOR);
    for verts in &anim.subsectors {
        for i in 0..verts.len() {
            let a = verts[i];
            let b = verts[(i + 1) % verts.len()];
            push_line(lines, a, b, ANIM_SUBSECTOR_THICKNESS * pr, subsector);
        }
    }
    let dot_half = ANIM_SEG_VERTEX_PX * pr * 0.5;
    for &(p, colour) in &anim.seg_verts {
        push_marker(markers, p, dot_half, rgba_f32(colour));
    }
}

/// Emit current step's seg lines and AABBs (transient; vertices kept by [`draw_kept`]).
fn draw_step_segs(
    anim: &BspAnim,
    lines: &mut Vec<LineInst>,
    segs: &[([f32; 2], [f32; 2], Color)],
    boxes: &[([f32; 2], [f32; 2], Color)],
) {
    let pr = anim.pixel_ratio;
    let halo = rgba_f32(anim.aabb_halo);
    for &(min, max, colour) in boxes {
        let pad = ANIM_AABB_PAD_WORLD;
        let x0 = min[0].min(max[0]) - pad;
        let x1 = min[0].max(max[0]) + pad;
        let y0 = min[1].min(max[1]) - pad;
        let y1 = min[1].max(max[1]) + pad;
        for (thick, colour) in [
            (ANIM_AABB_HALO_THICKNESS * pr, halo),
            (ANIM_AABB_CORE_THICKNESS * pr, rgba_f32(colour)),
        ] {
            push_line(lines, [x0, y0], [x1, y0], thick, colour);
            push_line(lines, [x1, y0], [x1, y1], thick, colour);
            push_line(lines, [x1, y1], [x0, y1], thick, colour);
            push_line(lines, [x0, y1], [x0, y0], thick, colour);
        }
    }
    for &(p0, p1, colour) in segs {
        push_line(lines, p0, p1, ANIM_SEG_THICKNESS * pr, rgba_f32(colour));
    }
}

/// Index past this step's events (next partition + its subsectors).
fn partition_step_end(events: &[BuildEvent], start: usize) -> usize {
    let mut seen_partition = false;
    for (offset, ev) in events[start..].iter().enumerate() {
        if matches!(ev, BuildEvent::PartitionChosen { .. }) {
            if seen_partition {
                return start + offset;
            }
            seen_partition = true;
        }
    }
    events.len()
}

/// Extend segment from midpoint to total length `span`. No-op for zero-length.
fn extend_to_bounds(a: [f32; 2], b: [f32; 2], span: f32) -> ([f32; 2], [f32; 2]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    if dx == 0.0 && dy == 0.0 {
        return (a, b);
    }
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = (dx / len, dy / len);
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    (
        [mid[0] - ux * span, mid[1] - uy * span],
        [mid[0] + ux * span, mid[1] + uy * span],
    )
}

/// Bounding-box diagonal of all event geometry; 0.0 for empty.
fn events_world_span(events: &[BuildEvent]) -> f32 {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut consider = |p: [f32; 2]| {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    };
    for ev in events {
        if let BuildEvent::SegsSplit {
            left,
            right,
            ..
        } = ev
        {
            for seg in left.iter().chain(right) {
                consider(seg[0]);
                consider(seg[1]);
            }
        }
    }
    if min_x > max_x {
        return 0.0;
    }
    let (w, h) = (max_x - min_x, max_y - min_y);
    (w * w + h * h).sqrt()
}

/// Vary brightness by index to keep adjacent same-side segs distinguishable.
fn shade(base: Color, index: usize) -> Color {
    const FACTORS: [f32; 4] = [1.0, 0.78, 0.6, 0.88];
    let f = FACTORS[index % FACTORS.len()];
    let s = |c: u8| (c as f32 * f).round().clamp(0.0, 255.0) as u8;
    [s(base[0]), s(base[1]), s(base[2]), base[3]]
}

fn luminance(c: Color) -> f32 {
    (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0
}

fn with_alpha(c: Color, alpha: u8) -> Color {
    [c[0], c[1], c[2], alpha]
}

pub fn clear(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    {
        let state = &mut *shared.borrow_mut();
        if state.bsp_anim.is_none() {
            return;
        }
        ANIM_TIMER.with(Timer::stop);
        state.bsp_anim = None;
        state.wgpu.set_overlay(&[], &[]);
    }
    repaint_canvas(ui, shared);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim() -> BspAnim {
        BspAnim {
            events: Vec::new(),
            next: 0,
            instant: false,
            keep_all: true,
            pixel_ratio: 1.0,
            world_span: 1000.0,
            divlines: vec![([0.0, 0.0], [50.0, -50.0])],
            subsectors: vec![vec![[10.0, -10.0], [40.0, -10.0], [40.0, -40.0]]],
            seg_verts: vec![([20.0, -20.0], [0x20, 0xb0, 0x30, 0xff])],
            divline: [0x90, 0x90, 0x90, 0xb0],
            aabb_halo: [0x00, 0x00, 0x00, 0xd0],
        }
    }

    #[test]
    fn kept_geometry_emits_lines_and_dots() {
        let a = anim();
        let mut lines = Vec::new();
        let mut markers = Vec::new();
        draw_kept(&a, &mut lines, &mut markers);
        assert_eq!(lines.len(), 4);
        assert_eq!(markers.len(), 1);
    }

    #[test]
    fn step_segs_emit_boxes_and_seglines() {
        let a = anim();
        let mut lines = Vec::new();
        let segs = vec![([0.0, 0.0], [10.0, -10.0], [0xe0, 0x70, 0x10, 0xff])];
        let boxes = vec![([0.0, -20.0], [30.0, 0.0], [0xe0, 0x20, 0x20, 0xff])];
        draw_step_segs(&a, &mut lines, &segs, &boxes);
        assert_eq!(lines.len(), 8 + 1);
    }
}
