//! Flat exposed-face list for GPU voxel rendering.
//!
//! Where [`super::slices`] builds span-encoded columns for the CPU rasterizer
//! (skip blanks at fragment time), this emits one face per exposed voxel side —
//! geometry-time blank skipping, the form a GPU wants. Each face carries its
//! grid position, normal axis/sign, and a pre-resolved palette colour, so the
//! shader is a trivial `colour * light` with no per-fragment span walk.

use super::kvx::VoxelModel;
use super::slices::{Axis, Dir, is_exposed, map_coords};

/// One exposed voxel face.
///
/// Built once per model, uploaded as GPU instance data.
/// Holds the palette index (not a resolved colour) so the cache survives a gamma
/// change — the renderer resolves `palette[pal_idx]` at upload.
pub struct VoxelFace {
    /// Voxel grid coordinate (x, y, z). KVX dims cap at 1024, so `u16` fits.
    pub pos: [u16; 3],
    /// Face normal axis: 0 = X, 1 = Y, 2 = Z.
    pub axis: u8,
    /// Which side along `axis` the face points: -1 (neg) or +1 (pos).
    pub sign: i8,
    /// Doom-palette colour index for this voxel.
    pub pal_idx: u8,
}

/// Walk the model and emit a face for every exposed voxel side. Geometry +
/// palette index only; colour resolution happens at GPU upload.
pub fn generate_faces(model: &VoxelModel) -> Vec<VoxelFace> {
    let mut faces = Vec::new();
    for axis_i in 0..3u8 {
        let axis = match axis_i {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };
        let (ds, us, vs) = axis_params(axis, model.xsiz, model.ysiz, model.zsiz);
        for d in 0..ds {
            for u in 0..us {
                for v in 0..vs {
                    for (dir, sign) in [(Dir::Neg, -1i8), (Dir::Pos, 1i8)] {
                        if !is_exposed(model, axis, dir, d, u, v) {
                            continue;
                        }
                        let (x, y, z) = map_coords(axis, d, u, v);
                        faces.push(VoxelFace {
                            pos: [x as u16, y as u16, z as u16],
                            axis: axis_i,
                            sign,
                            pal_idx: model.get(x, y, z),
                        });
                    }
                }
            }
        }
    }
    faces
}

/// (depth, u, v) extents for an axis pair, matching [`super::slices`].
fn axis_params(axis: Axis, xs: u32, ys: u32, zs: u32) -> (u32, u32, u32) {
    match axis {
        Axis::X => (xs, ys, zs),
        Axis::Y => (ys, xs, zs),
        Axis::Z => (zs, xs, ys),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::kvx::VoxelModel;

    #[test]
    fn ammoa_emits_faces() {
        let data = std::fs::read(test_utils::kvx_path("ammoa.kvx")).expect("ammoa.kvx not found");
        let model = VoxelModel::load(&data).expect("failed to parse");
        let faces = generate_faces(&model);
        assert!(!faces.is_empty(), "no faces emitted");
        for f in &faces {
            assert!(f.axis < 3);
            assert!(f.sign == -1 || f.sign == 1);
        }
    }

    #[test]
    fn buried_voxel_emits_no_face() {
        // 3x3x3 solid block: the centre voxel (1,1,1) is enclosed on all 6
        // sides, so it must not contribute any face.
        let xsiz = 3;
        let ysiz = 3;
        let zsiz = 3;
        let grid = vec![0u8; (xsiz * ysiz * zsiz) as usize];
        let model = VoxelModel {
            xsiz,
            ysiz,
            zsiz,
            xpivot: 0.0,
            ypivot: 0.0,
            zpivot: 0.0,
            grid,
            palette: None,
        };
        let faces = generate_faces(&model);
        let centre = faces.iter().any(|f| f.pos == [1, 1, 1]);
        assert!(!centre, "buried centre voxel must not expose a face");
        // The shell (26 outer voxels) still exposes its outward faces.
        assert!(!faces.is_empty());
    }
}
