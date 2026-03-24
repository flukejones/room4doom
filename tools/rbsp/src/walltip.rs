//! Wall-tip precomputation for sector assignment at vertices.
//!
//! For each vertex, a sorted list of WallTip entries records which sectors
//! adjoin the vertex at each angle. Used to assign sectors to seg-less
//! subsectors by querying "what sector is at angle θ from vertex V".

use crate::types::{Vertex, LineDefAccess, SideDefAccess, WadLineDef, WadSideDef, WallTip};

/// Build wall-tip lists for all vertices. Returns one Vec<WallTip> per vertex,
/// sorted by angle ascending.
pub fn build_wall_tips(
    linedefs: &[WadLineDef],
    sidedefs: &[WadSideDef],
    vertices: &[Vertex],
    num_vertices: usize,
) -> Vec<Vec<WallTip>> {
    let mut tips: Vec<Vec<WallTip>> = vec![Vec::new(); num_vertices];

    for ld in linedefs {
        let v1 = ld.start_vertex_idx();
        let v2 = ld.end_vertex_idx();
        if v1 >= num_vertices || v2 >= num_vertices {
            continue;
        }

        let dx = vertices[v2].x - vertices[v1].x;
        let dy = vertices[v2].y - vertices[v1].y;
        let angle_v1 = (dy as f64).atan2(dx as f64);
        let angle_v2 = (-(dy as f64)).atan2(-(dx as f64));

        let front_sector = ld.front_sidedef_idx().map(|i| sidedefs[i].sector_idx());
        let back_sector = ld.back_sidedef_idx().map(|i| sidedefs[i].sector_idx());

        tips[v1].push(WallTip {
            angle: angle_v1 as f64,
            front: front_sector,
            back: back_sector,
        });
        tips[v2].push(WallTip {
            angle: angle_v2 as f64,
            front: back_sector,
            back: front_sector,
        });
    }

    for tip_list in &mut tips {
        tip_list.sort_by(|a, b| a.angle.partial_cmp(&b.angle).unwrap());
    }

    tips
}

/// Query which sector occupies angle θ at a given vertex.
/// Returns None if the vertex has no wall-tips (synthetic vertex).
pub fn wall_tip_sector_at(tips: &[WallTip], angle: f64) -> Option<u32> {
    if tips.is_empty() {
        return None;
    }

    for i in 0..tips.len() {
        let next_i = (i + 1) % tips.len();
        let curr_angle = tips[i].angle;
        let next_angle = tips[next_i].angle;

        let in_range = if next_i == 0 {
            angle >= curr_angle || angle < next_angle
        } else {
            angle >= curr_angle && angle < next_angle
        };

        if in_range {
            if let Some(s) = tips[i].front {
                return Some(s as u32);
            }
            if let Some(s) = tips[next_i].back {
                return Some(s as u32);
            }
        }
    }

    for tip in tips {
        if let Some(s) = tip.front {
            return Some(s as u32);
        }
        if let Some(s) = tip.back {
            return Some(s as u32);
        }
    }

    None
}

/// Copy wall-tips from a linedef's endpoints to a new split vertex.
pub fn copy_wall_tips_for_split(
    wall_tips: &mut Vec<Vec<WallTip>>,
    linedef: &WadLineDef,
    sidedefs: &[WadSideDef],
    vertices: &[Vertex],
    new_vertex_idx: usize,
) {
    while wall_tips.len() <= new_vertex_idx {
        wall_tips.push(Vec::new());
    }

    let v1 = linedef.start_vertex_idx();
    let v2 = linedef.end_vertex_idx();
    let dx = vertices[v2].x - vertices[v1].x;
    let dy = vertices[v2].y - vertices[v1].y;
    let angle_fwd = (dy as f64).atan2(dx as f64);
    let angle_rev = (-(dy as f64)).atan2(-(dx as f64));

    let front_sector = linedef
        .front_sidedef_idx()
        .map(|i| sidedefs[i].sector_idx());
    let back_sector = linedef.back_sidedef_idx().map(|i| sidedefs[i].sector_idx());

    let tips = &mut wall_tips[new_vertex_idx];
    tips.push(WallTip {
        angle: angle_fwd as f64,
        front: front_sector,
        back: back_sector,
    });
    tips.push(WallTip {
        angle: angle_rev as f64,
        front: back_sector,
        back: front_sector,
    });
    tips.sort_by(|a, b| a.angle.partial_cmp(&b.angle).unwrap());
}
