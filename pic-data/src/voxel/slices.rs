/// Double-sided slice-quad generation from a VoxelModel.
///
/// For each of 3 axis pairs (X, Y, Z), iterate slices along the perpendicular
/// axis. Each slice stores both negative and positive face columns — the
/// renderer picks the visible side based on the viewing angle.
use super::kvx::VoxelModel;

pub struct VoxelSpan {
    pub start: u16,
    pub pixels: Vec<u8>,
}

pub struct VoxelColumn {
    pub spans: Vec<VoxelSpan>,
    pub skip_cols: u16,
}

/// Double-sided slice quad: one depth plane with neg and pos face textures.
pub struct VoxelSliceQuad {
    pub depth: f32,
    pub min_u: u16,
    pub min_v: u16,
    pub width: u16,
    pub height: u16,
    pub neg_columns: Vec<VoxelColumn>,
    pub pos_columns: Vec<VoxelColumn>,
}

/// 3 axis pairs indexed: 0=X, 1=Y, 2=Z
pub struct VoxelSlices {
    pub slices: [Vec<VoxelSliceQuad>; 3],
    pub xsiz: u32,
    pub ysiz: u32,
    pub zsiz: u32,
    pub xpivot: f32,
    pub ypivot: f32,
    pub zpivot: f32,
    pub angle_offset: f32,
    pub placed_spin: f32,
    pub dropped_spin: f32,
    pub face_player: bool,
}

/// Generate double-sided slice quads for all 3 axis pairs from a voxel model.
pub fn generate(model: &VoxelModel) -> VoxelSlices {
    let (xs, ys, zs) = (model.xsiz, model.ysiz, model.zsiz);

    let slices = [
        gen_axis_pair(model, Axis::X),
        gen_axis_pair(model, Axis::Y),
        gen_axis_pair(model, Axis::Z),
    ];

    VoxelSlices {
        slices,
        xsiz: xs,
        ysiz: ys,
        zsiz: zs,
        xpivot: model.xpivot,
        ypivot: model.ypivot,
        zpivot: model.zpivot,
        angle_offset: 0.0,
        placed_spin: 0.0,
        dropped_spin: 0.0,
        face_player: false,
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy)]
enum Dir {
    Neg,
    Pos,
}

fn axis_params(axis: Axis, xs: u32, ys: u32, zs: u32) -> (u32, u32, u32) {
    match axis {
        Axis::X => (xs, ys, zs),
        Axis::Y => (ys, xs, zs),
        Axis::Z => (zs, xs, ys),
    }
}

fn map_coords(axis: Axis, d: u32, u: u32, v: u32) -> (u32, u32, u32) {
    match axis {
        Axis::X => (d, u, v),
        Axis::Y => (u, d, v),
        Axis::Z => (u, v, d),
    }
}

/// Check if a voxel at (d, u, v) along `axis` is non-empty and has no
/// opaque neighbour in `dir` (i.e. its face is visible).
fn is_exposed(model: &VoxelModel, axis: Axis, dir: Dir, d: u32, u: u32, v: u32) -> bool {
    let (x, y, z) = map_coords(axis, d, u, v);
    if model.get(x, y, z) == 255 {
        return false;
    }
    let (xs, ys, zs) = (model.xsiz, model.ysiz, model.zsiz);
    match (axis, dir) {
        (Axis::X, Dir::Neg) => d == 0 || model.get(x.wrapping_sub(1), y, z) == 255,
        (Axis::X, Dir::Pos) => d == xs - 1 || model.get(x + 1, y, z) == 255,
        (Axis::Y, Dir::Neg) => d == 0 || model.get(x, y.wrapping_sub(1), z) == 255,
        (Axis::Y, Dir::Pos) => d == ys - 1 || model.get(x, y + 1, z) == 255,
        (Axis::Z, Dir::Neg) => d == 0 || model.get(x, y, z.wrapping_sub(1)) == 255,
        (Axis::Z, Dir::Pos) => d == zs - 1 || model.get(x, y, z + 1) == 255,
    }
}

/// Build span-encoded columns for one face direction of a slice plane.
/// Only exposed (visible-face) voxels are included; runs of consecutive
/// exposed voxels are merged into spans.
fn build_columns(
    model: &VoxelModel,
    axis: Axis,
    dir: Dir,
    d: u32,
    min_u: u32,
    max_u: u32,
    min_v: u32,
    max_v: u32,
) -> Vec<VoxelColumn> {
    let w = (max_u - min_u + 1) as usize;
    let mut columns = Vec::with_capacity(w);
    for u in min_u..=max_u {
        let mut spans = Vec::new();
        let mut span_start = None;
        let mut span_pixels = Vec::new();

        for v in min_v..=max_v {
            if is_exposed(model, axis, dir, d, u, v) {
                let (x, y, z) = map_coords(axis, d, u, v);
                if span_start.is_none() {
                    span_start = Some((v - min_v) as u16);
                }
                span_pixels.push(model.get(x, y, z));
            } else if span_start.is_some() {
                spans.push(VoxelSpan {
                    start: span_start.take().unwrap(),
                    pixels: std::mem::take(&mut span_pixels),
                });
            }
        }
        if let Some(start) = span_start {
            spans.push(VoxelSpan {
                start,
                pixels: span_pixels,
            });
        }
        columns.push(VoxelColumn {
            spans,
            skip_cols: 0,
        });
    }
    compute_skip_cols(&mut columns);
    columns
}

/// Precompute `skip_cols` for each column: the number of consecutive empty
/// columns following it (inclusive). Allows the renderer to skip blank
/// regions without iterating.
fn compute_skip_cols(columns: &mut [VoxelColumn]) {
    let mut dist = 0u16;
    for i in (0..columns.len()).rev() {
        if columns[i].spans.is_empty() {
            dist += 1;
            columns[i].skip_cols = dist;
        } else {
            dist = 0;
            columns[i].skip_cols = 0;
        }
    }
}

/// Find the tight (min_u, min_v, max_u, max_v) bounding box of exposed
/// voxels on a single slice plane. Returns `None` if no voxels are exposed.
fn find_bounds(
    model: &VoxelModel,
    axis: Axis,
    dir: Dir,
    d: u32,
    us: u32,
    vs: u32,
) -> Option<(u32, u32, u32, u32)> {
    let mut min_u = us;
    let mut min_v = vs;
    let mut max_u = 0u32;
    let mut max_v = 0u32;
    for u in 0..us {
        for v in 0..vs {
            if is_exposed(model, axis, dir, d, u, v) {
                min_u = min_u.min(u);
                min_v = min_v.min(v);
                max_u = max_u.max(u);
                max_v = max_v.max(v);
            }
        }
    }
    if max_u >= min_u {
        Some((min_u, min_v, max_u, max_v))
    } else {
        None
    }
}

/// Generate all slice quads for one axis pair. Each depth plane produces a
/// quad with neg and pos face columns. The bounding box is the union of
/// both sides.
fn gen_axis_pair(model: &VoxelModel, axis: Axis) -> Vec<VoxelSliceQuad> {
    let (ds, us, vs) = axis_params(axis, model.xsiz, model.ysiz, model.zsiz);
    let mut quads = Vec::new();

    for d in 0..ds {
        let neg_bounds = find_bounds(model, axis, Dir::Neg, d, us, vs);
        let pos_bounds = find_bounds(model, axis, Dir::Pos, d, us, vs);

        if neg_bounds.is_none() && pos_bounds.is_none() {
            continue;
        }

        // Union of both sides' bounding boxes
        let (min_u, min_v, max_u, max_v) = match (neg_bounds, pos_bounds) {
            (Some((a0, b0, a1, b1)), Some((c0, d0, c1, d1))) => {
                (a0.min(c0), b0.min(d0), a1.max(c1), b1.max(d1))
            }
            (Some(b), None) | (None, Some(b)) => b,
            _ => unreachable!(),
        };

        let w = max_u - min_u + 1;
        let h = max_v - min_v + 1;

        let neg_columns = if neg_bounds.is_some() {
            build_columns(model, axis, Dir::Neg, d, min_u, max_u, min_v, max_v)
        } else {
            Vec::new()
        };

        let pos_columns = if pos_bounds.is_some() {
            build_columns(model, axis, Dir::Pos, d, min_u, max_u, min_v, max_v)
        } else {
            Vec::new()
        };

        quads.push(VoxelSliceQuad {
            depth: d as f32,
            min_u: min_u as u16,
            min_v: min_v as u16,
            width: w as u16,
            height: h as u16,
            neg_columns,
            pos_columns,
        });
    }

    quads
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::kvx::VoxelModel;

    #[test]
    fn test_ammoa_slices() {
        let data = std::fs::read(test_utils::kvx_path("ammoa.kvx")).expect("ammoa.kvx not found");
        let model = VoxelModel::load(&data).expect("failed to parse");
        let slices = generate(&model);

        assert_eq!(slices.slices.len(), 3);

        let axis_names = ["X", "Y", "Z"];
        for (i, axis) in slices.slices.iter().enumerate() {
            assert!(!axis.is_empty(), "{} axis has no quads", axis_names[i]);

            let mut neg_px = 0usize;
            let mut pos_px = 0usize;
            for quad in axis {
                assert!(quad.width > 0);
                assert!(quad.height > 0);
                neg_px += quad
                    .neg_columns
                    .iter()
                    .flat_map(|c| c.spans.iter())
                    .map(|s| s.pixels.len())
                    .sum::<usize>();
                pos_px += quad
                    .pos_columns
                    .iter()
                    .flat_map(|c| c.spans.iter())
                    .map(|s| s.pixels.len())
                    .sum::<usize>();
            }
            eprintln!(
                "  {}: {} quads, neg={} px, pos={} px",
                axis_names[i],
                axis.len(),
                neg_px,
                pos_px
            );
        }
    }
}
