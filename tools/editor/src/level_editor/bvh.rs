//! AABB BVH over mesh triangles + thing billboard boxes for O(log n) pick gather; leaf bounds are camera-invariant, value edits refit in place, topology changes rebuild.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;

use crate::render::frame3d::Vert3D;

const LEAF_SIZE: usize = 4;
const PARALLEL_EPS: f32 = 1e-8;

thread_local! {
    /// Reused traversal stack; avoids a heap alloc each hover.
    static GATHER_STACK: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// `[half_w, half_h]` per thing. Box = `[x±r, y±r, z..z+2·hh]`, `r = max(hw,hh)`.
#[derive(Clone, Copy, Debug)]
pub struct ThingLeaf {
    pub id: u32,
    pub centre: [f32; 2],
    pub z: f32,
    pub half: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
pub enum Leaf {
    Tri {
        tri: usize,
    },
    Thing {
        id: u32,
        centre: [f32; 2],
        z: f32,
        half: [f32; 2],
    },
}

#[derive(Clone, Copy, Debug)]
struct Aabb {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
struct Node {
    bounds: Aabb,
    /// Interior: left child index. Leaf: `usize::MAX`.
    left: usize,
    /// Interior: right child index. Leaf: leaf-range start.
    right: usize,
    /// Leaf: range end; 0 for interior nodes.
    end: usize,
}

#[derive(Default)]
pub struct MeshBvh {
    nodes: Vec<Node>,
    order: Vec<Leaf>,
}

impl Aabb {
    fn empty() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    fn expand(&mut self, p: [f32; 3]) {
        for (a, &v) in p.iter().enumerate() {
            self.min[a] = self.min[a].min(v);
            self.max[a] = self.max[a].max(v);
        }
    }

    fn merge(&mut self, o: &Self) {
        for a in 0..3 {
            self.min[a] = self.min[a].min(o.min[a]);
            self.max[a] = self.max[a].max(o.max[a]);
        }
    }

    fn centre(&self) -> [f32; 3] {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
            0.5 * (self.min[2] + self.max[2]),
        ]
    }

    /// True when the ray comes within `radius` of the expanded box.
    fn ray_within(&self, origin: [f32; 3], dir: [f32; 3], radius: f32) -> bool {
        let min = [
            self.min[0] - radius,
            self.min[1] - radius,
            self.min[2] - radius,
        ];
        let max = [
            self.max[0] + radius,
            self.max[1] + radius,
            self.max[2] + radius,
        ];
        let mut t0 = 0.0f32;
        let mut t1 = f32::INFINITY;
        for a in 0..3 {
            if dir[a].abs() < PARALLEL_EPS {
                if origin[a] < min[a] || origin[a] > max[a] {
                    return false;
                }
            } else {
                let inv = 1.0 / dir[a];
                let mut near = (min[a] - origin[a]) * inv;
                let mut far = (max[a] - origin[a]) * inv;
                if near > far {
                    mem::swap(&mut near, &mut far);
                }
                t0 = t0.max(near);
                t1 = t1.min(far);
                if t0 > t1 {
                    return false;
                }
            }
        }
        t1 >= 0.0
    }
}

/// Bounds of one mesh triangle starting at vertex `at`.
fn tri_bounds(mesh: &[Vert3D], at: usize) -> Aabb {
    let mut b = Aabb::empty();
    if at + 3 <= mesh.len() {
        b.expand(mesh[at].pos);
        b.expand(mesh[at + 1].pos);
        b.expand(mesh[at + 2].pos);
    }
    b
}

/// Bounds of a thing billboard: ±max(hw,hh) so the box contains the quad for any camera orientation.
fn thing_bounds(centre: [f32; 2], z: f32, half: [f32; 2]) -> Aabb {
    let [hw, hh] = half;
    let r = hw.max(hh);
    let c = [centre[0], centre[1], z + hh];
    let mut b = Aabb::empty();
    b.expand([c[0] - r, c[1] - r, c[2] - r]);
    b.expand([c[0] + r, c[1] + r, c[2] + r]);
    b
}

fn leaf_bounds(leaf: &Leaf, mesh: &[Vert3D]) -> Aabb {
    match *leaf {
        Leaf::Tri {
            tri,
        } => tri_bounds(mesh, tri),
        Leaf::Thing {
            centre,
            z,
            half,
            ..
        } => thing_bounds(centre, z, half),
    }
}

impl MeshBvh {
    pub fn build(mesh: &[Vert3D], things: &[ThingLeaf]) -> Self {
        let mut items: Vec<(Aabb, Leaf)> = Vec::with_capacity(mesh.len() / 3 + things.len());
        let mut i = 0;
        while i + 3 <= mesh.len() {
            items.push((
                tri_bounds(mesh, i),
                Leaf::Tri {
                    tri: i,
                },
            ));
            i += 3;
        }
        for t in things {
            items.push((
                thing_bounds(t.centre, t.z, t.half),
                Leaf::Thing {
                    id: t.id,
                    centre: t.centre,
                    z: t.z,
                    half: t.half,
                },
            ));
        }
        let mut bvh = Self::default();
        if items.is_empty() {
            return bvh;
        }
        let mut idx: Vec<usize> = (0..items.len()).collect();
        bvh.build_node(&items, &mut idx);
        bvh
    }

    /// True when the leaf set still matches `mesh` + `things` — refit is valid, else rebuild.
    pub fn covers(&self, mesh: &[Vert3D], things: &[ThingLeaf]) -> bool {
        let tri_leaves = self
            .order
            .iter()
            .filter(|l| matches!(l, Leaf::Tri { .. }))
            .count();
        if tri_leaves != mesh.len() / 3 {
            return false;
        }
        let mut ids: Vec<u32> = self
            .order
            .iter()
            .filter_map(|l| match l {
                Leaf::Thing {
                    id,
                    ..
                } => Some(*id),
                Leaf::Tri {
                    ..
                } => None,
            })
            .collect();
        ids.sort_unstable();
        let mut want: Vec<u32> = things.iter().map(|t| t.id).collect();
        want.sort_unstable();
        ids == want
    }

    /// Recompute all AABBs bottom-up (children index after their parent, so a reverse sweep suffices).
    pub fn refit(&mut self, mesh: &[Vert3D], things: &[ThingLeaf]) {
        let by_id: HashMap<u32, &ThingLeaf> = things.iter().map(|t| (t.id, t)).collect();
        for leaf in &mut self.order {
            if let Leaf::Thing {
                id,
                centre,
                z,
                half,
            } = leaf
                && let Some(t) = by_id.get(id)
            {
                *centre = t.centre;
                *z = t.z;
                *half = t.half;
            }
        }
        for n in (0..self.nodes.len()).rev() {
            let node = self.nodes[n];
            let bounds = if node.left == usize::MAX {
                let mut b = Aabb::empty();
                for leaf in &self.order[node.right..node.end] {
                    b.merge(&leaf_bounds(leaf, mesh));
                }
                b
            } else {
                let mut b = self.nodes[node.left].bounds;
                b.merge(&self.nodes[node.right].bounds);
                b
            };
            self.nodes[n].bounds = bounds;
        }
    }

    fn build_node(&mut self, items: &[(Aabb, Leaf)], idx: &mut [usize]) -> usize {
        let mut bounds = Aabb::empty();
        for &k in idx.iter() {
            bounds.merge(&items[k].0);
        }
        let node = self.nodes.len();
        if idx.len() <= LEAF_SIZE {
            let start = self.order.len();
            for &k in idx.iter() {
                self.order.push(items[k].1);
            }
            self.nodes.push(Node {
                bounds,
                left: usize::MAX,
                right: start,
                end: self.order.len(),
            });
            return node;
        }
        let mut cmin = [f32::INFINITY; 3];
        let mut cmax = [f32::NEG_INFINITY; 3];
        for &k in idx.iter() {
            let c = items[k].0.centre();
            for a in 0..3 {
                cmin[a] = cmin[a].min(c[a]);
                cmax[a] = cmax[a].max(c[a]);
            }
        }
        let axis = (0..3)
            .max_by(|&a, &b| (cmax[a] - cmin[a]).total_cmp(&(cmax[b] - cmin[b])))
            .unwrap_or(0);
        idx.sort_unstable_by(|&a, &b| {
            items[a].0.centre()[axis].total_cmp(&items[b].0.centre()[axis])
        });
        let mid = idx.len() / 2;
        // Placeholder; child indices filled after recursion.
        self.nodes.push(Node {
            bounds,
            left: 0,
            right: 0,
            end: 0,
        });
        let (l, r) = idx.split_at_mut(mid);
        let left = self.build_node(items, l);
        let right = self.build_node(items, r);
        self.nodes[node] = Node {
            bounds,
            left,
            right,
            end: 0,
        };
        node
    }

    /// Collect leaves within `radius` of the ray; caller does precise tests.
    pub fn gather(&self, origin: [f32; 3], dir: [f32; 3], radius: f32, out: &mut Vec<Leaf>) {
        out.clear();
        if self.nodes.is_empty() {
            return;
        }
        GATHER_STACK.with_borrow_mut(|stack| {
            stack.clear();
            stack.push(0usize);
            while let Some(n) = stack.pop() {
                let node = self.nodes[n];
                if !node.bounds.ray_within(origin, dir, radius) {
                    continue;
                }
                if node.left == usize::MAX {
                    out.extend_from_slice(&self.order[node.right..node.end]);
                } else {
                    stack.push(node.left);
                    stack.push(node.right);
                }
            }
        });
    }

    /// Thing ids whose boxes overlap `[min, max]`.
    pub fn things_in_box(&self, min: [f32; 3], max: [f32; 3], out: &mut Vec<u32>) {
        out.clear();
        if self.nodes.is_empty() {
            return;
        }
        GATHER_STACK.with_borrow_mut(|stack| {
            stack.clear();
            stack.push(0usize);
            while let Some(n) = stack.pop() {
                let node = self.nodes[n];
                let b = node.bounds;
                if max[0] < b.min[0]
                    || min[0] > b.max[0]
                    || max[1] < b.min[1]
                    || min[1] > b.max[1]
                    || max[2] < b.min[2]
                    || min[2] > b.max[2]
                {
                    continue;
                }
                if node.left == usize::MAX {
                    for leaf in &self.order[node.right..node.end] {
                        if let Leaf::Thing {
                            id,
                            centre,
                            z,
                            half,
                        } = *leaf
                        {
                            let r = half[0].max(half[1]);
                            let c = [centre[0], centre[1], z + half[1]];
                            if c[0] + r < min[0]
                                || c[0] - r > max[0]
                                || c[1] + r < min[1]
                                || c[1] - r > max[1]
                                || c[2] + r < min[2]
                                || c[2] - r > max[2]
                            {
                                continue;
                            }
                            out.push(id);
                        }
                    }
                } else {
                    stack.push(node.left);
                    stack.push(node.right);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_editor::pick3d::pick_mesh;
    use crate::render::frame3d::build_surface;
    use crate::render::triangulate::build_sector_tris;
    use std::collections::HashMap;

    fn e1m1_mesh() -> Vec<Vert3D> {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let tris = build_sector_tris(&map);
        build_surface(&map, &tris, &HashMap::new()).0
    }

    /// Gathered leaves must contain the triangle `pick_mesh` hits.
    #[test]
    fn bvh_gather_contains_brute_force_hit() {
        let mesh = e1m1_mesh();
        let bvh = MeshBvh::build(&mesh, &[]);
        let origin = [1056.0, -3616.0, 1.0e5];
        let dir = [0.0, 0.0, -1.0];
        let hit = pick_mesh(&mesh, origin, dir).expect("ray hits floor");
        let mut leaves = Vec::new();
        bvh.gather(origin, dir, 8.0, &mut leaves);
        let has = leaves
            .iter()
            .any(|l| matches!(l, Leaf::Tri { tri } if *tri == hit.tri));
        assert!(has, "gather includes the brute-force hit triangle");
    }

    /// Every `pick_mesh` hit over a coarse E1M1 grid of downward rays must be in the gather set.
    fn assert_gather_never_prunes(bvh: &MeshBvh, mesh: &[Vert3D], label: &str) {
        let mut leaves = Vec::new();
        let mut checked = 0;
        let dir = [0.0, 0.0, -1.0];
        for gx in 0..12 {
            for gy in 0..12 {
                let x = -700.0 + gx as f32 * 380.0;
                let y = -4800.0 + gy as f32 * 230.0;
                let origin = [x, y, 1.0e5];
                let Some(hit) = pick_mesh(mesh, origin, dir) else {
                    continue;
                };
                bvh.gather(origin, dir, 8.0, &mut leaves);
                assert!(
                    leaves
                        .iter()
                        .any(|l| matches!(l, Leaf::Tri { tri } if *tri == hit.tri)),
                    "{label} dropped the pierced triangle at ({x},{y})"
                );
                checked += 1;
            }
        }
        assert!(checked > 20, "sampled enough interior rays ({checked})");
    }

    #[test]
    fn gather_never_prunes_the_true_hit() {
        let mesh = e1m1_mesh();
        let bvh = MeshBvh::build(&mesh, &[]);
        assert_gather_never_prunes(&bvh, &mesh, "built tree");
    }

    /// A refit tree over an in-place-mutated mesh keeps every pierced triangle (reconcile-path picks).
    #[test]
    fn refit_gather_never_prunes_on_mutated_mesh() {
        let mut mesh = e1m1_mesh();
        let mut bvh = MeshBvh::build(&mesh, &[]);
        // Nudge every vertex in a band of the map, as a big multi-select move would.
        for v in mesh
            .iter_mut()
            .filter(|v| v.pos[0] > 500.0 && v.pos[0] < 1500.0)
        {
            v.pos[0] += 96.0;
            v.pos[1] -= 48.0;
        }
        assert!(bvh.covers(&mesh, &[]), "leaf set unchanged by a value edit");
        bvh.refit(&mesh, &[]);
        assert_gather_never_prunes(&bvh, &mesh, "refit tree");
    }

    #[test]
    fn things_in_box_filters_by_volume() {
        let mesh = e1m1_mesh();
        let things = [
            ThingLeaf {
                id: 0,
                centre: [100.0, 100.0],
                z: 0.0,
                half: [16.0, 16.0],
            },
            ThingLeaf {
                id: 1,
                centre: [9000.0, 9000.0],
                z: 0.0,
                half: [16.0, 16.0],
            },
        ];
        let bvh = MeshBvh::build(&mesh, &things);
        let mut out = Vec::new();
        bvh.things_in_box([50.0, 50.0, -64.0], [150.0, 150.0, 64.0], &mut out);
        assert!(out.contains(&0), "thing inside the box is returned");
        assert!(!out.contains(&1), "thing outside the box is skipped");
    }
}
