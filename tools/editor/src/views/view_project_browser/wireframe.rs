//! Top-down linedef wireframe as SVG path-data strings, grouped by role.
//! Viewbox Y negated (map Y is up).

use std::fmt::Write as _;

use editor_core::EditorMap;

/// Viewbox inset fraction so linedefs don't touch the border.
const VIEWBOX_PAD: f32 = 0.04;

/// SVG path-data per role + shared viewbox (negated-Y space).
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct WirePaths {
    pub one_sided: String,
    pub two_sided: String,
    pub special: String,
    pub vb_x: f32,
    pub vb_y: f32,
    pub vb_w: f32,
    pub vb_h: f32,
}

/// Build path commands + viewbox; empty map → unit viewbox.
pub(super) fn build(map: &EditorMap) -> WirePaths {
    let Some((min, max)) = bounds(map) else {
        return WirePaths {
            vb_w: 1.0,
            vb_h: 1.0,
            ..Default::default()
        };
    };

    let w = (max[0] - min[0]).max(1.0);
    let h = (max[1] - min[1]).max(1.0);
    let pad = w.max(h) * VIEWBOX_PAD;
    let mut paths = WirePaths {
        vb_x: min[0] - pad,
        vb_y: -max[1] - pad,
        vb_w: w + 2.0 * pad,
        vb_h: h + 2.0 * pad,
        ..Default::default()
    };

    for line in &map.lines {
        let (Some(v1), Some(v2)) = (
            map.vertices.get(line.v1 as usize),
            map.vertices.get(line.v2 as usize),
        ) else {
            continue;
        };
        let target = if line.special != 0 {
            &mut paths.special
        } else if line.back.is_some() {
            &mut paths.two_sided
        } else {
            &mut paths.one_sided
        };
        let _ = write!(target, "M {} {} L {} {} ", v1.x, -v1.y, v2.x, -v2.y);
    }
    paths
}

fn bounds(map: &EditorMap) -> Option<([f32; 2], [f32; 2])> {
    let mut it = map.vertices.iter();
    let first = it.next()?;
    let mut min = [first.x, first.y];
    let mut max = [first.x, first.y];
    for v in it {
        min[0] = min[0].min(v.x);
        min[1] = min[1].min(v.y);
        max[0] = max[0].max(v.x);
        max[1] = max[1].max(v.y);
    }
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{LineDef, LineFlags, SideDef, Vertex};

    fn vtx(x: f32, y: f32) -> Vertex {
        Vertex {
            x,
            y,
        }
    }

    fn side() -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Default::default(),
            bottom_tex: Default::default(),
            middle_tex: Default::default(),
            sector: Some(0),
        }
    }

    #[test]
    fn empty_map_unit_viewbox() {
        let p = build(&EditorMap::default());
        assert_eq!((p.vb_w, p.vb_h), (1.0, 1.0));
        assert!(p.one_sided.is_empty());
    }

    #[test]
    fn box_emits_one_sided_segments_and_bounds() {
        let map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(64.0, 64.0),
                vtx(0.0, 64.0),
            ],
            lines: vec![
                LineDef {
                    v1: 0,
                    v2: 1,
                    flags: LineFlags::empty(),
                    special: 0,
                    tag: 0,
                    front: side(),
                    back: None,
                },
                LineDef {
                    v1: 1,
                    v2: 2,
                    flags: LineFlags::empty(),
                    special: 0,
                    tag: 0,
                    front: side(),
                    back: None,
                },
            ],
            ..Default::default()
        };
        let p = build(&map);
        // Y negated: top edge is near -max_y, with inset padding.
        let pad = 64.0 * VIEWBOX_PAD;
        assert!((p.vb_x - (0.0 - pad)).abs() < 1e-3);
        assert!((p.vb_w - (64.0 + 2.0 * pad)).abs() < 1e-3);
        assert!((p.vb_y - (-64.0 - pad)).abs() < 1e-3);
        assert!((p.vb_h - (64.0 + 2.0 * pad)).abs() < 1e-3);
        // Two one-sided segments; no two-sided or special.
        assert_eq!(p.one_sided.matches('M').count(), 2);
        assert!(p.two_sided.is_empty() && p.special.is_empty());
    }
}
