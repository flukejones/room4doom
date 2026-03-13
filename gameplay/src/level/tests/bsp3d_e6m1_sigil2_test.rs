#[cfg(test)]
mod tests {
    use super::super::{DOOM_WAD, SIGIL2_WAD, load_map_with_pwad};

    /// Diagnostic test for subsector 2587 in E6M1 (sigil2.wad).
    /// Subsector 2587 produces malformed floor/ceiling polygons.
    /// This test prints the polygon vertex data to identify the root cause.
    #[test]
    fn test_e6m1_subsector_2587_polygon() {
        let map = load_map_with_pwad(DOOM_WAD, SIGIL2_WAD, "E6M1");

        let bsp3d = &map.bsp_3d;

        assert!(
            bsp3d.subsector_leaves.len() > 2587,
            "Map must have at least 2588 subsectors, got {}",
            bsp3d.subsector_leaves.len()
        );

        let leaf = &bsp3d.subsector_leaves[2587];
        let vertices = &bsp3d.vertices;

        println!("=== Subsector 2587 diagnostic (E6M1, sigil2.wad) ===");
        println!("Sector ID: {}", leaf.sector_id);
        println!("Total polygons: {}", leaf.polygons.len());
        println!("Floor polygon indices: {:?}", leaf.floor_polygons);
        println!("Ceiling polygon indices: {:?}", leaf.ceiling_polygons);

        for &fp_idx in &leaf.floor_polygons {
            let poly = &leaf.polygons[fp_idx];
            println!("\n--- Floor polygon [{}] ---", fp_idx);
            println!("  Vertex count: {}", poly.vertices.len());
            println!("  Moves: {}", poly.moves);
            for (i, &vi) in poly.vertices.iter().enumerate() {
                println!("  v[{}] = idx:{} pos:{:?}", i, vi, vertices[vi]);
            }

            for i in 0..poly.vertices.len() {
                for j in (i + 1)..poly.vertices.len() {
                    if poly.vertices[i] == poly.vertices[j] {
                        println!(
                            "  DUPLICATE vertex index at positions {} and {}: idx={}",
                            i, j, poly.vertices[i]
                        );
                    }
                }
            }

            println!("  Fan triangles:");
            let n = poly.vertices.len();
            let mut degenerate_count = 0;
            for i in 1..n.saturating_sub(1) {
                let a = poly.vertices[0];
                let b = poly.vertices[i];
                let c = poly.vertices[i + 1];
                let pa = vertices[a];
                let pb = vertices[b];
                let pc = vertices[c];
                let ab = pb - pa;
                let ac = pc - pa;
                let area2 = ab.cross(ac).length();
                let degenerate = a == b || b == c || a == c || area2 < 1e-4;
                if degenerate {
                    degenerate_count += 1;
                }
                println!(
                    "    tri({},{},{}) area2={:.6} {}",
                    a,
                    b,
                    c,
                    area2,
                    if degenerate { "DEGENERATE" } else { "" }
                );
            }

            if degenerate_count == 0 {
                println!("  Floor polygon looks valid.");
            }
        }

        for &cp_idx in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[cp_idx];
            println!("\n--- Ceiling polygon [{}] ---", cp_idx);
            println!("  Vertex count: {}", poly.vertices.len());
            println!("  Moves: {}", poly.moves);
            for (i, &vi) in poly.vertices.iter().enumerate() {
                println!("  v[{}] = idx:{} pos:{:?}", i, vi, vertices[vi]);
            }

            for i in 0..poly.vertices.len() {
                for j in (i + 1)..poly.vertices.len() {
                    if poly.vertices[i] == poly.vertices[j] {
                        println!(
                            "  DUPLICATE vertex index at positions {} and {}: idx={}",
                            i, j, poly.vertices[i]
                        );
                    }
                }
            }

            let n = poly.vertices.len();
            let mut degenerate_count = 0;
            for i in 1..n.saturating_sub(1) {
                let a = poly.vertices[0];
                let b = poly.vertices[i];
                let c = poly.vertices[i + 1];
                let pa = vertices[a];
                let pb = vertices[b];
                let pc = vertices[c];
                let ab = pb - pa;
                let ac = pc - pa;
                let area2 = ab.cross(ac).length();
                let degenerate = a == b || b == c || a == c || area2 < 1e-4;
                if degenerate {
                    degenerate_count += 1;
                }
                println!(
                    "    tri({},{},{}) area2={:.6} {}",
                    a,
                    b,
                    c,
                    area2,
                    if degenerate { "DEGENERATE" } else { "" }
                );
            }

            if degenerate_count == 0 {
                println!("  Ceiling polygon looks valid.");
            }
        }

        println!("\n--- Segments for subsector 2587 ---");
        let all_subsectors = &map.subsectors;
        if let Some(subsector) = all_subsectors.get(2587) {
            println!(
                "  start_seg={} seg_count={}",
                subsector.start_seg, subsector.seg_count
            );
            let start = subsector.start_seg as usize;
            let end = start + subsector.seg_count as usize;
            for i in start..end {
                let seg = &map.segments[i];
                println!(
                    "  seg[{}]: v1={:?} v2={:?} linedef={} frontsec={} backsec={:?}",
                    i,
                    *seg.v1,
                    *seg.v2,
                    seg.linedef.num,
                    seg.frontsector.num,
                    seg.backsector.as_ref().map(|s| s.num)
                );
            }
        }

        let mut total_degenerate = 0;
        let vertices = &bsp3d.vertices;
        for &fp_idx in &leaf.floor_polygons {
            let poly = &leaf.polygons[fp_idx];
            let n = poly.vertices.len();
            for i in 1..n.saturating_sub(1) {
                let a = poly.vertices[0];
                let b = poly.vertices[i];
                let c = poly.vertices[i + 1];
                if a == b || b == c || a == c {
                    total_degenerate += 1;
                    continue;
                }
                let pa = vertices[a];
                let pb = vertices[b];
                let pc = vertices[c];
                let area2 = (pb - pa).cross(pc - pa).length();
                if area2 < 1e-4 {
                    total_degenerate += 1;
                }
            }
        }

        assert_eq!(
            total_degenerate, 0,
            "Subsector 2587 floor has {} degenerate triangle(s)",
            total_degenerate
        );
    }

    /// Check that no subsector in E6M1 has duplicate vertex indices in floor
    /// polygons. This catches the spare-vertex degenerate polygon issue
    /// across the whole map.
    #[test]
    fn test_e6m1_no_degenerate_floor_triangles() {
        let map = load_map_with_pwad(DOOM_WAD, SIGIL2_WAD, "E6M1");

        let bsp3d = &map.bsp_3d;
        let vertices = &bsp3d.vertices;

        let mut failures = Vec::new();

        for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
            for &fp_idx in &leaf.floor_polygons {
                let poly = &leaf.polygons[fp_idx];
                let n = poly.vertices.len();
                for i in 1..n.saturating_sub(1) {
                    let a = poly.vertices[0];
                    let b = poly.vertices[i];
                    let c = poly.vertices[i + 1];
                    let degenerate = if a == b || b == c || a == c {
                        true
                    } else {
                        let pa = vertices[a];
                        let pb = vertices[b];
                        let pc = vertices[c];
                        (pb - pa).cross(pc - pa).length() < 1e-4
                    };
                    if degenerate {
                        failures.push((ssid, fp_idx, i, a, b, c));
                    }
                }
            }
        }

        if !failures.is_empty() {
            for (ssid, fp_idx, tri_i, a, b, c) in &failures {
                let area2 = if *a == *b || *b == *c || *a == *c {
                    0.0f32
                } else {
                    let pa = vertices[*a];
                    let pb = vertices[*b];
                    let pc = vertices[*c];
                    (pb - pa).cross(pc - pa).length()
                };
                println!(
                    "Degenerate: subsector={} floor_poly={} tri={} indices=({},{},{}) area2={:.6} pos=({:?},{:?},{:?})",
                    ssid, fp_idx, tri_i, a, b, c, area2, vertices[*a], vertices[*b], vertices[*c]
                );
            }
        }

        assert!(
            failures.is_empty(),
            "{} degenerate floor triangle(s) found across E6M1",
            failures.len()
        );
    }
}
