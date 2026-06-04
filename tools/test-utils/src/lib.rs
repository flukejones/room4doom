use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use glam::Vec3;
use level::{BSP3D, LevelData, SurfacePolygon};
use wad::WadData;

/// Path to the shareware Doom1 WAD included in the repo under `data/`.
pub fn doom1_wad_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("data/doom1.wad")
}

/// Directory for user-supplied WADs (~/doom/).
fn user_wad_dir() -> PathBuf {
    dirs::home_dir().expect("no home dir").join("doom")
}

pub fn doom_wad_path() -> PathBuf {
    user_wad_dir().join("doom.wad")
}

pub fn doom2_wad_path() -> PathBuf {
    user_wad_dir().join("doom2.wad")
}

pub fn sigil_wad_path() -> PathBuf {
    user_wad_dir().join("sigil.wad")
}

pub fn sigil2_wad_path() -> PathBuf {
    user_wad_dir().join("sigil2.wad")
}

pub fn sunder_wad_path() -> PathBuf {
    user_wad_dir().join("sunder.wad")
}

/// Path to a KVX voxel file under ~/doom/cheello_voxels/voxels/.
pub fn kvx_path(name: &str) -> PathBuf {
    user_wad_dir().join("cheello_voxels/voxels").join(name)
}

/// Load a map with a no-op flat lookup (BSP/PVS tests don't need real flats).
pub fn load_map(wad_path: &Path, map_name: &str) -> LevelData {
    let wad = WadData::new(wad_path);
    let mut map = LevelData::default();
    map.load(map_name, |_| None, &wad, None, None);
    map
}

/// Load a map from a base WAD with a PWAD merged.
pub fn load_map_with_pwad(base_wad: &Path, pwad: &Path, map_name: &str) -> LevelData {
    let mut wad = WadData::new(base_wad);
    wad.add_file(pwad.into());
    let mut map = LevelData::default();
    map.load(map_name, |_| None, &wad, None, None);
    map
}

pub fn eviternity_wad_path() -> PathBuf {
    user_wad_dir().join("Eviternity.wad")
}

/// Floor or ceiling — selects which surface polygons a helper operates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Floor,
    Ceiling,
}

/// Signed shoelace area of a polygon's XY projection. Positive = CCW (floor),
/// negative = CW (ceiling).
pub fn shoelace(poly: &SurfacePolygon, verts: &[Vec3]) -> f32 {
    let n = poly.vertices.len();
    (0..n)
        .map(|i| {
            let a = verts[poly.vertices[i]];
            let b = verts[poly.vertices[(i + 1) % n]];
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// All vertex indices used by a sector's floor or ceiling polygons.
pub fn collect_sector_vertices(
    bsp3d: &BSP3D,
    sector_id: usize,
    surface: Surface,
) -> BTreeSet<usize> {
    bsp3d.sector_subsectors[sector_id]
        .iter()
        .flat_map(|&ssid| {
            let leaf = &bsp3d.subsector_leaves[ssid];
            let polys = match surface {
                Surface::Floor => &leaf.floor_polygons,
                Surface::Ceiling => &leaf.ceiling_polygons,
            };
            polys
                .iter()
                .flat_map(|&pi| bsp3d.polygons[pi].vertices.iter().copied())
        })
        .collect()
}

/// Linedef IDs of every segment bordering `sector_id` (front or back side).
pub fn collect_border_linedefs(map: &LevelData, sector_id: usize) -> BTreeSet<usize> {
    map.segments
        .iter()
        .filter(|s| {
            s.frontsector.num == sector_id as i32
                || s.backsector
                    .as_ref()
                    .is_some_and(|b| b.num == sector_id as i32)
        })
        .map(|s| s.linedef.num)
        .collect()
}

/// Assert floor/ceiling polygon winding and normals across a whole map.
///
/// Every non-sky subsector must have exactly one floor (normal +Z, positive
/// shoelace) and one ceiling (normal −Z, negative shoelace), with the smaller
/// polygon's XY a subset of the larger (mover sectors gain boundary vertices).
/// Panics with a list of all failures.
pub fn assert_floor_ceiling_normals(map: &LevelData) {
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        if leaf.polygon_indices.is_empty() {
            continue;
        }
        let has_floor = !leaf.floor_polygons.is_empty();
        let has_ceil = !leaf.ceiling_polygons.is_empty();
        // Sky subsectors may lack a floor or ceiling.
        if !has_floor || !has_ceil {
            continue;
        }
        if leaf.floor_polygons.len() != 1 {
            failures.push(format!(
                "ss={ssid}: {} floor polygons",
                leaf.floor_polygons.len()
            ));
            continue;
        }
        if leaf.ceiling_polygons.len() != 1 {
            failures.push(format!(
                "ss={ssid}: {} ceiling polygons",
                leaf.ceiling_polygons.len()
            ));
            continue;
        }

        let floor = &bsp3d.polygons[leaf.floor_polygons[0]];
        let ceil = &bsp3d.polygons[leaf.ceiling_polygons[0]];

        if floor.normal != Vec3::new(0.0, 0.0, 1.0) {
            failures.push(format!("ss={ssid}: floor normal {:?}", floor.normal));
        }
        if ceil.normal != Vec3::new(0.0, 0.0, -1.0) {
            failures.push(format!("ss={ssid}: ceiling normal {:?}", ceil.normal));
        }
        if floor.vertices.len() < 3 || ceil.vertices.len() < 3 {
            failures.push(format!("ss={ssid}: degenerate floor/ceiling (< 3 verts)"));
            continue;
        }

        let floor_area = shoelace(floor, verts);
        let ceil_area = shoelace(ceil, verts);
        if floor_area <= 0.0 {
            failures.push(format!(
                "ss={ssid}: floor shoelace={floor_area:.2} (expected > 0)"
            ));
        }
        if ceil_area >= 0.0 {
            failures.push(format!(
                "ss={ssid}: ceiling shoelace={ceil_area:.2} (expected < 0)"
            ));
        }

        // Smaller polygon's XY must be a subset of the larger (epsilon: mover
        // sectors separate vertices to slightly different positions).
        let xy = |p: &SurfacePolygon| -> Vec<(f32, f32)> {
            p.vertices
                .iter()
                .map(|&vi| (verts[vi].x, verts[vi].y))
                .collect()
        };
        let floor_xy = xy(floor);
        let ceil_xy = xy(ceil);
        let (smaller, larger) = if floor_xy.len() <= ceil_xy.len() {
            (&floor_xy, &ceil_xy)
        } else {
            (&ceil_xy, &floor_xy)
        };
        let subset = smaller.iter().all(|s| {
            larger
                .iter()
                .any(|l| (s.0 - l.0).abs() < 2.0 && (s.1 - l.1).abs() < 2.0)
        });
        if !subset {
            failures.push(format!("ss={ssid}: floor/ceiling XY mismatch"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Load a map with flat name tracking so sky sectors get correct flat indices.
/// Returns (LevelData, Option<sky_flat_index>).
///
/// Uses manual lump scan for flat list (LumpIter has a bug with multi-chunk
/// flat sections where IWAD flats get dropped when PWAD has its own section).
pub fn load_map_with_flats(wad: &WadData, map_name: &str) -> (LevelData, Option<usize>) {
    let mut flats = Vec::new();
    let mut in_flats = false;
    for l in wad.lumps() {
        if l.name == "F_START" || l.name == "FF_START" {
            in_flats = true;
            continue;
        }
        if l.name == "F_END" || l.name == "FF_END" {
            in_flats = false;
            continue;
        }
        if in_flats && !l.data.is_empty() {
            flats.push(l.name.clone());
        }
    }
    // Deduplicate: last occurrence wins (PWAD override)
    let mut deduped: Vec<String> = Vec::new();
    for name in flats.iter().rev() {
        if !deduped.iter().any(|n| n == name) {
            deduped.push(name.clone());
        }
    }
    deduped.reverse();

    let sky_num = deduped.iter().position(|n| n == "F_SKY1");
    let flat_lookup = move |name: &str| -> Option<usize> { deduped.iter().position(|n| n == name) };
    let mut map = LevelData::default();
    map.load(map_name, flat_lookup, wad, sky_num, None);
    (map, sky_num)
}
