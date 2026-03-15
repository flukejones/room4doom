use egui::{Color32, Mesh, Pos2, Stroke, Vec2 as EVec2};
use glam::Vec2;
use map_data::{
    MapData, PVS2D, Portals, PvsCluster, PvsData, PvsFile, PvsView2D, RenderPvs, SurfaceKind, is_subsector, pvs_load_from_cache, subsector_index
};
use std::any::Any;
use std::sync::Arc;
use wad::WadData;

/// Height range (map units) at which brightness spans the full
/// [BRIGHTNESS_MIN, BRIGHTNESS_MAX] band. Smaller ranges produce a narrower
/// band centered on BRIGHTNESS_CENTER.
const BRIGHTNESS_REF_RANGE: f32 = 1024.0;

/// Brightness band limits and center.
const BRIGHTNESS_MIN: f32 = 0.2;
const BRIGHTNESS_MAX: f32 = 1.0;
const BRIGHTNESS_CENTER: f32 = 0.5;

// ── PVS backend selection ──

/// PVS implementation to build, passed from the CLI into the viewer.
pub enum PvsInput {
    /// Standard 2D portal-flood PVS.
    Pvs2D(PVS2D),
    /// Cluster-based PVS (skeleton).
    Cluster(PvsCluster),
}

/// Tracks which PVS backend is active so `rebuild_pvs` can repeat the same
/// build.
#[derive(Clone, Copy, PartialEq)]
enum PvsBackendKind {
    Pvs2D,
    Cluster,
}

// ── Extracted data types (owned, no raw pointers) ──

/// A portal between two subsectors, stored for rendering.
struct ViewPortal {
    ss_a: usize,
    ss_b: usize,
    v1: Vec2,
    v2: Vec2,
}

pub struct ViewerData {
    map_name: String,
    min: Vec2,
    max: Vec2,
    linedefs: Vec<ViewLinedef>,
    sectors: Vec<ViewSector>,
    subsectors: Vec<ViewSubsector>,
    /// Portal graph — always populated (even when `--no-pvs`).
    portals: Vec<ViewPortal>,
    /// Visibility queries (backed by RenderPvs).
    pvs: Option<Box<dyn PvsData>>,
    /// Concrete PVS type for downcast access to mightsee.
    /// `None` when loaded from cache (no mightsee data available).
    pvs_any: Option<Box<dyn Any + Send>>,
    divlines: Vec<ViewDivline>,
    vertices: Vec<ViewVertex>,
    /// For each subsector, the indices into `divlines` on its root-to-leaf
    /// path.
    ss_divline_path: Vec<Vec<usize>>,
    /// Which PVS backend is active (used by rebuild to repeat the same build).
    backend: Option<PvsBackendKind>,
    /// Per-sector brightness value (0.0–1.0) derived from floor height rank.
    /// Indexed by sector ID. Rank-based so every distinct height level gets a
    /// distinct brightness step regardless of actual height spacing.
    floor_height_value: Vec<f32>,
    /// Per-sector average floor texture colour, computed from WAD flat data and
    /// palette. Indexed by sector ID.
    floor_texture_color: Vec<Color32>,
    /// Cache metadata for save/rebuild.
    map_hash: u64,
    iwad_path: String,
    pwad_paths: Vec<String>,
}

struct ViewLinedef {
    index: usize,
    v1: Vec2,
    v2: Vec2,
    is_two_sided: bool,
    front_sector_id: usize,
    back_sector_id: Option<usize>,
    special: i16,
    tag: i16,
}

struct ViewSector {
    floor_height: f32,
    ceiling_height: f32,
    light_level: usize,
    special: i16,
    tag: i16,
}

struct ViewSubsector {
    /// Subsector index (matches BSP leaf index).
    index: usize,
    /// Owning sector, from `BSPLeaf3D.sector_id`.
    sector_id: usize,
    /// 2D carved polygon from `BSP3D.carved_polygons`.
    vertices: Vec<Vec2>,
    /// Precomputed 2D AABB min corner from `BSPLeaf3D.aabb`.
    aabb_min: Vec2,
    /// Precomputed 2D AABB max corner from `BSPLeaf3D.aabb`.
    aabb_max: Vec2,
    /// Unique linedef IDs from vertical surfaces in this leaf.
    /// Reserved for future texture-based colouring.
    #[allow(dead_code)]
    wall_linedef_ids: Vec<usize>,
}

struct ViewDivline {
    index: usize,
    origin: Vec2,
    dir: Vec2,
}

struct ViewVertex {
    index: usize,
    pos: Vec2,
    linedef_ids: Vec<usize>,
}

// ── Data extraction ──

pub fn extract_viewer_data(
    map_name: &str,
    map_data: &MapData,
    wad: &WadData,
    pvs: Option<PvsInput>,
    map_hash: u64,
    iwad_path: String,
    pwad_paths: Vec<String>,
) -> ViewerData {
    let extents = map_data.get_map_extents();

    let linedefs: Vec<ViewLinedef> = map_data
        .linedefs
        .iter()
        .enumerate()
        .map(|(i, ld)| ViewLinedef {
            index: i,
            v1: *ld.v1,
            v2: *ld.v2,
            is_two_sided: ld.backsector.is_some(),
            front_sector_id: ld.frontsector.num as usize,
            back_sector_id: ld.backsector.as_ref().map(|bs| bs.num as usize),
            special: ld.special,
            tag: ld.tag,
        })
        .collect();

    let sectors: Vec<ViewSector> = map_data
        .sectors
        .iter()
        .map(|s| ViewSector {
            floor_height: s.floorheight,
            ceiling_height: s.ceilingheight,
            light_level: s.lightlevel,
            special: s.special,
            tag: s.tag,
        })
        .collect();

    // Brightness centered on BRIGHTNESS_CENTER. The band width scales with
    // the map's actual height range relative to BRIGHTNESS_REF_RANGE.
    // Small variation → narrow band (e.g. 0.4–0.6).
    // Large variation → wide band (e.g. 0.2–0.8, clamped to [MIN, MAX]).
    let floor_height_value: Vec<f32> = {
        let min_h = sectors
            .iter()
            .map(|s| s.floor_height)
            .fold(f32::MAX, f32::min);
        let max_h = sectors
            .iter()
            .map(|s| s.floor_height)
            .fold(f32::MIN, f32::max);
        let range = max_h - min_h;
        let half_band = (BRIGHTNESS_MAX - BRIGHTNESS_MIN) * 0.5 * (range / BRIGHTNESS_REF_RANGE);
        sectors
            .iter()
            .map(|s| {
                if range <= 0.0 {
                    BRIGHTNESS_CENTER
                } else {
                    let t = (s.floor_height - min_h) / range; // 0..1
                    (BRIGHTNESS_CENTER - half_band + t * 2.0 * half_band)
                        .clamp(BRIGHTNESS_MIN, BRIGHTNESS_MAX)
                }
            })
            .collect()
    };

    // Average floor texture colour per sector from WAD flat data + palette.
    let floor_texture_color: Vec<Color32> = {
        let palette = wad.playpal_iter().next().unwrap();
        let flats: Vec<_> = wad.flats_iter().collect();
        map_data
            .sectors
            .iter()
            .map(|s| match flats.get(s.floorpic) {
                Some(f) if !f.data.is_empty() => {
                    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
                    let count = f.data.len() as u64;
                    for &pi in &f.data {
                        let c = palette.0[pi as usize];
                        r += ((c >> 16) & 0xFF) as u64;
                        g += ((c >> 8) & 0xFF) as u64;
                        b += (c & 0xFF) as u64;
                    }
                    Color32::from_rgb((r / count) as u8, (g / count) as u8, (b / count) as u8)
                }
                _ => Color32::from_rgb(128, 128, 128),
            })
            .collect()
    };

    // Build vertex list with linedef associations (by position key).
    let pos_key = |v: Vec2| -> (u32, u32) { (v.x.to_bits(), v.y.to_bits()) };
    let pos_to_vidx: std::collections::HashMap<(u32, u32), usize> = map_data
        .vertexes
        .iter()
        .enumerate()
        .map(|(i, &v)| (pos_key(v), i))
        .collect();
    // Some linedefs may reference vertices not in vertexes (BSP-split verts) —
    // ignore those.
    let mut vert_ld_ids: Vec<Vec<usize>> = vec![Vec::new(); map_data.vertexes.len()];
    for ld in &linedefs {
        if let Some(&vi) = pos_to_vidx.get(&pos_key(ld.v1)) {
            vert_ld_ids[vi].push(ld.index);
        }
        if let Some(&vi) = pos_to_vidx.get(&pos_key(ld.v2)) {
            vert_ld_ids[vi].push(ld.index);
        }
    }
    drop(pos_to_vidx);
    let vertices: Vec<ViewVertex> = map_data
        .vertexes
        .iter()
        .enumerate()
        .map(|(i, &v)| ViewVertex {
            index: i,
            pos: v,
            linedef_ids: std::mem::take(&mut vert_ld_ids[i]),
        })
        .collect();

    let bsp = &map_data.bsp_3d;
    let subsectors: Vec<ViewSubsector> = bsp
        .carved_polygons
        .iter()
        .enumerate()
        .zip(bsp.subsector_leaves.iter())
        .map(|((i, verts), leaf)| {
            let sector_id = leaf.sector_id;
            if verts.len() >= 3 {
                validate_polygon(i, sector_id, verts);
            }
            let mut wall_linedef_ids: Vec<usize> = leaf
                .polygons
                .iter()
                .filter_map(|p| match &p.surface_kind {
                    SurfaceKind::Vertical {
                        linedef_id,
                        ..
                    } => Some(*linedef_id),
                    _ => None,
                })
                .collect();
            wall_linedef_ids.sort_unstable();
            wall_linedef_ids.dedup();
            ViewSubsector {
                index: i,
                sector_id,
                vertices: verts.clone(),
                aabb_min: leaf.aabb.min.truncate(),
                aabb_max: leaf.aabb.max.truncate(),
                wall_linedef_ids,
            }
        })
        .collect();

    // Collect BSP divlines and compute root-to-leaf path for each subsector.
    let mut divlines = Vec::new();
    let mut ss_divline_path = vec![Vec::new(); map_data.subsectors.len()];
    collect_divline_paths(
        map_data.start_node,
        &map_data.nodes,
        &mut divlines,
        &mut Vec::new(),
        &mut ss_divline_path,
    );

    /// Extract `ViewPortal` vec from a `Portals` collection.
    fn portals_from_graph(graph: &Portals) -> Vec<ViewPortal> {
        graph
            .iter()
            .map(|p| ViewPortal {
                ss_a: p.subsector_a,
                ss_b: p.subsector_b,
                v1: p.v1,
                v2: p.v2,
            })
            .collect()
    }

    let (pvs_data, pvs_any, portals, backend): (
        Option<Box<dyn PvsData>>,
        Option<Box<dyn Any + Send>>,
        Vec<ViewPortal>,
        Option<PvsBackendKind>,
    ) = match pvs {
        Some(PvsInput::Pvs2D(pvs2d)) => {
            let portals = portals_from_graph(pvs2d.portals_2d());
            let render = pvs2d.clone_render_pvs();
            (
                Some(Box::new(render)),
                Some(Box::new(pvs2d)),
                portals,
                Some(PvsBackendKind::Pvs2D),
            )
        }
        Some(PvsInput::Cluster(cluster)) => {
            let portals = portals_from_graph(cluster.portals_2d());
            let render = cluster.clone_render_pvs();
            (
                Some(Box::new(render)),
                Some(Box::new(cluster)),
                portals,
                Some(PvsBackendKind::Cluster),
            )
        }
        None => {
            // Build portals only (no PVS flood) so the portal layer still works
            // in --no-pvs mode.
            let graph = Portals::build(
                map_data.start_node,
                &map_data.nodes,
                &map_data.subsectors,
                &map_data.segments,
                &map_data.bsp_3d,
            );
            let portals = portals_from_graph(&graph);
            (None, None, portals, None)
        }
    };

    ViewerData {
        map_name: map_name.to_owned(),
        min: extents.min_vertex,
        max: extents.max_vertex,
        linedefs,
        sectors,
        subsectors,
        vertices,
        portals,
        pvs: pvs_data,
        pvs_any,
        divlines,
        ss_divline_path,
        backend,
        map_hash,
        iwad_path,
        pwad_paths,
        floor_height_value,
        floor_texture_color,
    }
}

/// Walk BSP tree, recording divlines and building root-to-leaf paths for each
/// subsector.
fn collect_divline_paths(
    node_id: u32,
    nodes: &[map_data::map_defs::Node],
    divlines: &mut Vec<ViewDivline>,
    path: &mut Vec<usize>,
    ss_paths: &mut [Vec<usize>],
) {
    if is_subsector(node_id) {
        let ss_id = if node_id == u32::MAX {
            0
        } else {
            subsector_index(node_id)
        };
        if ss_id < ss_paths.len() {
            ss_paths[ss_id] = path.clone();
        }
        return;
    }
    let Some(node) = nodes.get(node_id as usize) else {
        return;
    };
    let divline_idx = divlines.len();
    divlines.push(ViewDivline {
        index: node_id as usize,
        origin: Vec2::new(node.xy.x, node.xy.y),
        dir: Vec2::new(node.delta.x, node.delta.y),
    });
    path.push(divline_idx);
    collect_divline_paths(node.children[0], nodes, divlines, path, ss_paths);
    collect_divline_paths(node.children[1], nodes, divlines, path, ss_paths);
    path.pop();
}

// ── Debug query tools ──

/// Drag mode for debug tools: right-drag = line probe, shift+drag = rect
/// select.
#[derive(Clone, Copy, PartialEq)]
enum DragTool {
    None,
    LineProbe { start: Vec2 },
    RectSelect { start: Vec2 },
}

impl MapViewerApp {
    /// Output query text: print to stdout and copy to clipboard (with map/wad
    /// header).
    fn output_query(&self, ctx: &egui::Context, text: &str) {
        print!("{text}");
        let mut clipboard = String::new();
        clipboard.push_str(&format!("map: {}\n", self.data.map_name));
        clipboard.push_str(&format!("iwad: {}\n", self.data.iwad_path));
        for p in &self.data.pwad_paths {
            clipboard.push_str(&format!("pwad: {}\n", p));
        }
        clipboard.push_str(text);
        ctx.copy_text(clipboard);
    }

    /// Build query text for all entities intersecting the line from `a` to `b`.
    fn query_line(&self, a: Vec2, b: Vec2) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- LINE_PROBE ---");
        let _ = writeln!(
            buf,
            "line: ({:.1},{:.1}) -> ({:.1},{:.1})",
            a.x, a.y, b.x, b.y
        );

        // Subsectors intersecting line
        let _ = writeln!(buf, "# subsectors");
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if line_intersects_polygon(a, b, &ss.vertices) {
                let s = &self.data.sectors[ss.sector_id];
                let _ = writeln!(
                    buf,
                    "ss={} sector={} floor={} ceil={}",
                    ss.index, ss.sector_id, s.floor_height, s.ceiling_height,
                );
            }
        }

        // Linedefs intersecting line
        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if segments_intersect(a, b, ld.v1, ld.v2) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        // Divlines intersecting the query line (check finite segment of divline within
        // map bounds)
        let _ = writeln!(buf, "# divlines");
        for dl in &self.data.divlines {
            let len = dl.dir.length();
            if len < 1e-6 {
                continue;
            }
            let norm = dl.dir / len;
            let extent = 32768.0;
            let dl_a = dl.origin - norm * extent;
            let dl_b = dl.origin + norm * extent;
            if segments_intersect(a, b, dl_a, dl_b) {
                let _ = writeln!(
                    buf,
                    "divline: node={} origin=({:.1},{:.1}) dir=({:.1},{:.1})",
                    dl.index, dl.origin.x, dl.origin.y, dl.dir.x, dl.dir.y
                );
            }
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }

    /// Build query text for all entities contained in the rectangle from `lo`
    /// to `hi`.
    fn query_rect(&self, lo: Vec2, hi: Vec2) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- RECT_SELECT ---");
        let _ = writeln!(
            buf,
            "rect: ({:.1},{:.1}) -> ({:.1},{:.1})",
            lo.x, lo.y, hi.x, hi.y
        );

        // Subsectors with centroid inside rect
        let _ = writeln!(buf, "# subsectors");
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if polygon_inside_rect(&ss.vertices, lo, hi) {
                let s = &self.data.sectors[ss.sector_id];
                let _ = writeln!(
                    buf,
                    "ss={} sector={} floor={} ceil={} verts={}",
                    ss.index,
                    ss.sector_id,
                    s.floor_height,
                    s.ceiling_height,
                    ss.vertices.len()
                );
            }
        }

        // Linedefs with both vertices inside rect
        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if point_in_rect(ld.v1, lo, hi) && point_in_rect(ld.v2, lo, hi) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        // Divlines whose origin is inside rect
        let _ = writeln!(buf, "# divlines");
        for dl in &self.data.divlines {
            if point_in_rect(dl.origin, lo, hi) {
                let _ = writeln!(
                    buf,
                    "divline: node={} origin=({:.1},{:.1}) dir=({:.1},{:.1})",
                    dl.index, dl.origin.x, dl.origin.y, dl.dir.x, dl.dir.y
                );
            }
        }

        // Unique sectors referenced by contained subsectors
        let _ = writeln!(buf, "# sectors");
        let mut seen_sectors = std::collections::BTreeSet::new();
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if polygon_inside_rect(&ss.vertices, lo, hi) {
                seen_sectors.insert(ss.sector_id);
            }
        }
        for &sid in &seen_sectors {
            let s = &self.data.sectors[sid];
            let _ = writeln!(
                buf,
                "sector={} floor={} ceil={} light={} special={} tag={}",
                sid, s.floor_height, s.ceiling_height, s.light_level, s.special, s.tag,
            );
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }

    /// Build query text for all data in a given sector (triggered by
    /// Cmd+click).
    fn query_sector(&self, sector_id: usize) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- SECTOR_QUERY ---");
        let s = &self.data.sectors[sector_id];
        let _ = writeln!(
            buf,
            "sector={} floor={} ceil={} light={} special={} tag={}",
            sector_id, s.floor_height, s.ceiling_height, s.light_level, s.special, s.tag,
        );

        // All subsectors belonging to this sector
        let _ = writeln!(buf, "# subsectors");
        for ss in &self.data.subsectors {
            if ss.sector_id != sector_id {
                continue;
            }
            let _ = writeln!(buf, "ss={} verts={}", ss.index, ss.vertices.len());
        }

        // All linedefs with this sector as front or back
        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if ld.front_sector_id == sector_id || ld.back_sector_id == Some(sector_id) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }
}

// ── Viewer app ──

struct ViewState {
    offset: EVec2,
    zoom: f32,
    show_linedefs: bool,
    show_sectors: bool,
    show_subsectors: bool,
    show_pvs: bool,
    show_mightsee: bool,
    show_aabb: bool,
    show_divlines: bool,
    show_all_portals: bool,
    show_pvs_portals: bool,
    show_map_polygon_edges: bool,
    show_vertices: bool,
    selected_subsector: Option<usize>,
    hovered_linedef: Option<usize>,
    hovered_subsector: Option<usize>,
    hovered_vertex: Option<usize>,
    /// When true, selection is pinned and does not follow the mouse.
    pinned: bool,
    is_dragging: bool,
    drag_tool: DragTool,
    status_msg: Option<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            offset: EVec2::ZERO,
            zoom: 1.0,
            show_linedefs: true,
            show_sectors: true,
            show_subsectors: false,
            show_pvs: true,
            show_mightsee: false,
            show_aabb: false,
            show_divlines: true,
            show_all_portals: false,
            show_pvs_portals: false,
            show_map_polygon_edges: false,
            show_vertices: false,
            selected_subsector: None,
            hovered_linedef: None,
            hovered_subsector: None,
            hovered_vertex: None,
            pinned: false,
            is_dragging: false,
            drag_tool: DragTool::None,
            status_msg: None,
        }
    }
}

struct MapViewerApp {
    data: ViewerData,
    state: ViewState,
    map_center: Vec2,
}

impl MapViewerApp {
    fn new(data: ViewerData) -> Self {
        let map_center = (data.min + data.max) * 0.5;
        Self {
            data,
            state: ViewState::default(),
            map_center,
        }
    }

    fn save_pvs(&mut self) {
        let Some(any) = &self.data.pvs_any else {
            self.state.status_msg =
                Some("No PVS data to save (load from cache has no portal data)".into());
            return;
        };
        if let Some(pvs2d) = any.downcast_ref::<PVS2D>() {
            match pvs2d.save_to_cache(&self.data.map_name, self.data.map_hash) {
                Ok(()) => {
                    let path_display =
                        RenderPvs::cache_path(&self.data.map_name, self.data.map_hash)
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "<unknown path>".into());
                    let msg = format!("PVS saved to {path_display}");
                    log::info!("{msg}");
                    self.state.status_msg = Some(msg);
                }
                Err(e) => {
                    self.state.status_msg = Some(format!("Save failed: {e}"));
                }
            }
        } else if any.downcast_ref::<PvsCluster>().is_some() {
            self.state.status_msg = Some("PvsCluster does not support cache save yet".into());
        } else {
            self.state.status_msg = Some("Internal error: unknown PVS backend".into());
        }
    }

    fn load_pvs_cache(&mut self) {
        let ss_count = self.data.subsectors.len();
        match pvs_load_from_cache(&self.data.map_name, self.data.map_hash, ss_count) {
            Some(render_pvs) => {
                self.data.pvs = Some(Box::new(render_pvs));
                self.data.pvs_any = None; // no portal data from cache
                let msg = "PVS loaded from cache (no portal data — use Rebuild for portals)";
                log::info!("{msg}");
                self.state.status_msg = Some(msg.into());
            }
            None => {
                self.state.status_msg = Some("No cached PVS found for this map".into());
            }
        }
    }

    fn rebuild_pvs(&mut self) {
        use map_data::MapData;
        use std::path::PathBuf;

        log::info!("Rebuilding PVS...");
        self.state.status_msg = Some("Rebuilding PVS...".into());

        let wad_path: PathBuf = self.data.iwad_path.clone().into();
        let mut wad = wad::WadData::new(&wad_path);
        for pwad in &self.data.pwad_paths {
            wad.add_file(pwad.into());
        }

        let flat_lookup: std::collections::HashMap<String, usize> = wad
            .flats_iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        let mut map_data = Box::pin(MapData::default());
        unsafe { map_data.as_mut().get_unchecked_mut() }.load(
            &self.data.map_name,
            |name| flat_lookup.get(name).copied(),
            &wad,
            None,
        );

        let backend = self.data.backend.unwrap_or(PvsBackendKind::Pvs2D);

        fn portals_to_view(graph: &Portals) -> Vec<ViewPortal> {
            graph
                .iter()
                .map(|p| ViewPortal {
                    ss_a: p.subsector_a,
                    ss_b: p.subsector_b,
                    v1: p.v1,
                    v2: p.v2,
                })
                .collect()
        }

        let ss_count = map_data.subsectors.len();
        let (render, portals, pvs_any) = match backend {
            PvsBackendKind::Pvs2D => {
                let built = PVS2D::build(
                    &map_data.subsectors,
                    &map_data.segments,
                    &map_data.bsp_3d,
                    &map_data.nodes,
                    map_data.start_node,
                    false,
                );
                let portals = portals_to_view(built.portals_2d());
                let render = built.clone_render_pvs();
                let any: Box<dyn Any + Send> = Box::new(built);
                (render, portals, any)
            }
            PvsBackendKind::Cluster => {
                let built = PvsCluster::build(
                    &map_data.subsectors,
                    &map_data.segments,
                    &map_data.bsp_3d,
                    &map_data.sectors,
                    &map_data.linedefs,
                    &map_data.nodes,
                    map_data.start_node,
                );
                let portals = portals_to_view(built.portals_2d());
                let render = built.clone_render_pvs();
                let any: Box<dyn Any + Send> = Box::new(built);
                (render, portals, any)
            }
        };

        self.data.portals = portals;
        let vis_count = (0..ss_count)
            .map(|s| render.get_visible_subsectors(s).len())
            .sum::<usize>();
        self.data.pvs = Some(Box::new(render));
        self.data.pvs_any = Some(pvs_any);
        let msg = format!("PVS rebuilt: {} visible pairs", vis_count);
        log::info!("{msg}");
        self.state.status_msg = Some(msg);
    }

    #[inline]
    fn map_to_screen(&self, map_pos: Vec2, vc: Pos2) -> Pos2 {
        Pos2::new(
            vc.x + (map_pos.x - self.map_center.x) * self.state.zoom + self.state.offset.x,
            vc.y - (map_pos.y - self.map_center.y) * self.state.zoom + self.state.offset.y,
        )
    }

    #[inline]
    fn screen_to_map(&self, sp: Pos2, vc: Pos2) -> Vec2 {
        Vec2::new(
            (sp.x - vc.x - self.state.offset.x) / self.state.zoom + self.map_center.x,
            -(sp.y - vc.y - self.state.offset.y) / self.state.zoom + self.map_center.y,
        )
    }

    fn fit_zoom(&mut self, viewport_size: EVec2) {
        let map_w = self.data.max.x - self.data.min.x;
        let map_h = self.data.max.y - self.data.min.y;
        if map_w > 0.0 && map_h > 0.0 {
            self.state.zoom = (viewport_size.x / map_w).min(viewport_size.y / map_h) * 0.9;
        }
    }

    fn handle_input(&mut self, response: &egui::Response) {
        let modifiers = response.ctx.input(|i| i.modifiers);
        let ctrl = modifiers.command; // Cmd on macOS, Ctrl on others
        let shift = modifiers.shift;

        // Ctrl+drag: line probe. Ctrl+Shift+drag: rect select.
        let is_primary_drag = response.dragged_by(egui::PointerButton::Primary);
        let primary_stopped = response.drag_stopped_by(egui::PointerButton::Primary);

        if is_primary_drag && ctrl && self.state.drag_tool == DragTool::None {
            if let Some(pos) = response.interact_pointer_pos() {
                let vc = response.rect.center();
                let map_pos = self.screen_to_map(pos, vc);
                if shift {
                    self.state.drag_tool = DragTool::RectSelect {
                        start: map_pos,
                    };
                } else {
                    self.state.drag_tool = DragTool::LineProbe {
                        start: map_pos,
                    };
                }
            }
        }

        if primary_stopped && self.state.drag_tool != DragTool::None {
            if let Some(pos) = response.interact_pointer_pos() {
                let vc = response.rect.center();
                let end = self.screen_to_map(pos, vc);
                match self.state.drag_tool {
                    DragTool::LineProbe {
                        start,
                    } => {
                        if (end - start).length() > 1.0 {
                            let text = self.query_line(start, end);
                            self.output_query(&response.ctx, &text);
                        }
                    }
                    DragTool::RectSelect {
                        start,
                    } => {
                        let lo = Vec2::new(start.x.min(end.x), start.y.min(end.y));
                        let hi = Vec2::new(start.x.max(end.x), start.y.max(end.y));
                        if (hi.x - lo.x) > 1.0 && (hi.y - lo.y) > 1.0 {
                            let text = self.query_rect(lo, hi);
                            self.output_query(&response.ctx, &text);
                        }
                    }
                    DragTool::None => {}
                }
            }
            self.state.drag_tool = DragTool::None;
        }

        // Plain left-drag / middle-drag: pan (only when no tool active)
        if self.state.drag_tool == DragTool::None {
            if is_primary_drag || response.dragged_by(egui::PointerButton::Middle) {
                self.state.offset += response.drag_delta();
                self.state.is_dragging = true;
            }
        }
        if primary_stopped || response.drag_stopped_by(egui::PointerButton::Middle) {
            self.state.is_dragging = false;
        }

        let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(pointer_pos) = response.hover_pos() {
                let vc = response.rect.center();
                let mouse_map = self.screen_to_map(pointer_pos, vc);
                self.state.zoom *= 1.002_f32.powf(scroll);
                self.state.zoom = self.state.zoom.clamp(0.01, 200.0);
                let new_screen = self.map_to_screen(mouse_map, vc);
                self.state.offset.x += pointer_pos.x - new_screen.x;
                self.state.offset.y += pointer_pos.y - new_screen.y;
            }
        }

        if response.clicked() && !self.state.is_dragging {
            if ctrl {
                // Cmd+click: print all data for the hovered sector
                if let Some(ss_id) = self.state.hovered_subsector {
                    if let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_id) {
                        let text = self.query_sector(ss.sector_id);
                        self.output_query(&response.ctx, &text);
                    }
                }
            } else if let Some(hovered) = self.state.hovered_subsector {
                if self.state.pinned && self.state.selected_subsector == Some(hovered) {
                    // Click same region while pinned: unpin (re-enable follow)
                    self.state.pinned = false;
                } else {
                    // Click: pin selection to this subsector
                    self.state.selected_subsector = Some(hovered);
                    self.state.pinned = true;
                }
            }
        }
    }

    fn update_hover(&mut self, vc: Pos2, pointer_pos: Option<Pos2>) {
        self.state.hovered_linedef = None;
        self.state.hovered_subsector = None;
        self.state.hovered_vertex = None;

        let Some(pointer) = pointer_pos else {
            return;
        };
        let mouse_map = self.screen_to_map(pointer, vc);

        let threshold = 5.0 / self.state.zoom;
        let mut best_dist = threshold;
        for ld in &self.data.linedefs {
            let dist = point_to_segment_dist(mouse_map, ld.v1, ld.v2);
            if dist < best_dist {
                best_dist = dist;
                self.state.hovered_linedef = Some(ld.index);
            }
        }

        if self.state.show_vertices {
            let vx_threshold = 6.0 / self.state.zoom;
            let mut best_vx_dist = vx_threshold;
            for vx in &self.data.vertices {
                let dist = (mouse_map - vx.pos).length();
                if dist < best_vx_dist {
                    best_vx_dist = dist;
                    self.state.hovered_vertex = Some(vx.index);
                }
            }
        }

        for ss in &self.data.subsectors {
            if ss.vertices.len() >= 3 && point_in_polygon(mouse_map, &ss.vertices) {
                self.state.hovered_subsector = Some(ss.index);
                break;
            }
        }

        // When not pinned, selection follows the mouse
        if !self.state.pinned {
            self.state.selected_subsector = self.state.hovered_subsector;
        }
    }

    fn draw_layers(&mut self, painter: &egui::Painter, vc: Pos2) {
        if self.state.show_sectors {
            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    continue;
                }
                let base = self.data.floor_texture_color[ss.sector_id];
                let v = self.data.floor_height_value[ss.sector_id];
                let peak = base.r().max(base.g()).max(base.b()).max(1) as f32;
                let scale = v * 255.0 / peak;
                let color = Color32::from_rgb(
                    (base.r() as f32 * scale).min(255.0) as u8,
                    (base.g() as f32 * scale).min(255.0) as u8,
                    (base.b() as f32 * scale).min(255.0) as u8,
                );
                let points: Vec<Pos2> = ss
                    .vertices
                    .iter()
                    .map(|&v| self.map_to_screen(v, vc))
                    .collect();
                painter.add(filled_polygon(&points, color));
            }
        }

        if self.state.show_subsectors {
            let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 0, 120));
            for ss in &self.data.subsectors {
                let n = ss.vertices.len();
                for i in 0..n {
                    let p1 = self.map_to_screen(ss.vertices[i], vc);
                    let p2 = self.map_to_screen(ss.vertices[(i + 1) % n], vc);
                    painter.line_segment([p1, p2], stroke);
                }
            }
        }

        if self.state.show_map_polygon_edges {
            let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 60, 60, 200));
            for ss in &self.data.subsectors {
                let n = ss.vertices.len();
                for i in 0..n {
                    let p1 = self.map_to_screen(ss.vertices[i], vc);
                    let p2 = self.map_to_screen(ss.vertices[(i + 1) % n], vc);
                    painter.line_segment([p1, p2], stroke);
                }
            }
        }

        if self.state.show_aabb {
            let sel = self.state.selected_subsector;
            let pvs_active = self.state.show_pvs && sel.is_some();

            let default_stroke =
                Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 100, 255, 60));
            let selected_stroke =
                Stroke::new(2.5, Color32::from_rgba_unmultiplied(255, 255, 0, 255));
            let pvs_stroke = Stroke::new(1.5, Color32::from_rgba_unmultiplied(80, 255, 80, 255));

            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    continue;
                }

                let is_selected = sel == Some(ss.index);
                let is_pvs = pvs_active
                    && self
                        .data
                        .pvs
                        .as_ref()
                        .map_or(false, |pvs| pvs.is_visible(sel.unwrap(), ss.index));

                let stroke = if is_selected {
                    selected_stroke
                } else if is_pvs {
                    pvs_stroke
                } else {
                    default_stroke
                };

                let mn = ss.aabb_min;
                let mx = ss.aabb_max;
                let tl = self.map_to_screen(Vec2::new(mn.x, mx.y), vc);
                let tr = self.map_to_screen(Vec2::new(mx.x, mx.y), vc);
                let br = self.map_to_screen(Vec2::new(mx.x, mn.y), vc);
                let bl = self.map_to_screen(Vec2::new(mn.x, mn.y), vc);
                painter.line_segment([tl, tr], stroke);
                painter.line_segment([tr, br], stroke);
                painter.line_segment([br, bl], stroke);
                painter.line_segment([bl, tl], stroke);
            }
        }

        if self.state.show_linedefs {
            for ld in &self.data.linedefs {
                let p1 = self.map_to_screen(ld.v1, vc);
                let p2 = self.map_to_screen(ld.v2, vc);
                let (width, color) = if self.state.hovered_linedef == Some(ld.index) {
                    (3.0, Color32::YELLOW)
                } else if !ld.is_two_sided {
                    (1.5, Color32::WHITE)
                } else {
                    (0.5, Color32::from_gray(120))
                };
                painter.line_segment([p1, p2], Stroke::new(width, color));
            }
        }

        if self.state.show_vertices {
            for vx in &self.data.vertices {
                let sp = self.map_to_screen(vx.pos, vc);
                let is_hovered = self.state.hovered_vertex == Some(vx.index);
                let (radius, color) = if is_hovered {
                    (5.0, Color32::from_rgb(255, 220, 50))
                } else {
                    (2.5, Color32::from_rgba_unmultiplied(180, 180, 255, 200))
                };
                painter.circle_filled(sp, radius, color);
                if is_hovered {
                    painter.circle_stroke(
                        sp,
                        radius + 2.0,
                        Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 220, 50, 120)),
                    );
                }
            }
        }

        if self.state.show_all_portals && !self.data.portals.is_empty() {
            let w = (self.state.zoom * 0.5).clamp(1.0, 6.0);
            let glow = Stroke::new(w + 4.0, Color32::from_rgba_unmultiplied(0, 150, 255, 40));
            let core = Stroke::new(w, Color32::from_rgba_unmultiplied(0, 180, 255, 160));
            for portal in &self.data.portals {
                let p1 = self.map_to_screen(portal.v1, vc);
                let p2 = self.map_to_screen(portal.v2, vc);
                painter.line_segment([p1, p2], glow);
                painter.line_segment([p1, p2], core);
            }
        }

        // Mightsee overlay: coarse BFS superset, rendered before PVS so the
        // tighter PVS green shows on top. Only available when portal data is
        // present (i.e. after build(), not after load from cache).
        if self.state.show_mightsee {
            if let Some(sel) = self.state.selected_subsector {
                let mightsee_set: std::collections::HashSet<usize> = self
                    .data
                    .pvs_any
                    .as_ref()
                    .and_then(|a| {
                        if let Some(p) = a.downcast_ref::<PVS2D>() {
                            Some(p.get_mightsee_subsectors(sel))
                        } else if let Some(p) = a.downcast_ref::<PvsCluster>() {
                            Some(p.get_mightsee_subsectors(sel))
                        } else {
                            None
                        }
                    })
                    .map(|v| v.into_iter().collect())
                    .unwrap_or_default();
                if !mightsee_set.is_empty() {
                    let fill = Color32::from_rgba_unmultiplied(255, 160, 0, 35);
                    self.draw_visibility_layer(painter, vc, sel, fill, |ss_idx| {
                        mightsee_set.contains(&ss_idx)
                    });
                }
            }
        }

        if self.state.show_pvs {
            if let (Some(sel), Some(pvs)) = (self.state.selected_subsector, &self.data.pvs) {
                let fill = Color32::from_rgba_unmultiplied(60, 255, 60, 50);
                self.draw_visibility_layer(painter, vc, sel, fill, |ss_idx| {
                    pvs.is_visible(sel, ss_idx)
                });
            }
        }

        // PVS highlight portals: portals between any two visible subsectors.
        if self.state.show_pvs_portals {
            if let (Some(sel), Some(pvs)) = (self.state.selected_subsector, &self.data.pvs) {
                let visible: std::collections::HashSet<usize> =
                    pvs.get_visible_subsectors(sel).into_iter().collect();
                let w = (self.state.zoom * 0.5).clamp(1.0, 6.0);
                let glow = Stroke::new(w + 4.0, Color32::from_rgba_unmultiplied(255, 120, 0, 60));
                let core = Stroke::new(w, Color32::from_rgba_unmultiplied(255, 160, 0, 240));
                for portal in self
                    .data
                    .portals
                    .iter()
                    .filter(|p| visible.contains(&p.ss_a) && visible.contains(&p.ss_b))
                {
                    let p1 = self.map_to_screen(portal.v1, vc);
                    let p2 = self.map_to_screen(portal.v2, vc);
                    painter.line_segment([p1, p2], glow);
                    painter.line_segment([p1, p2], core);
                }
            }
        }

        if self.state.show_divlines {
            if let Some(sel) = self.state.selected_subsector {
                if let Some(path) = self.data.ss_divline_path.get(sel) {
                    for (depth, &dl_idx) in path.iter().enumerate() {
                        let dl = &self.data.divlines[dl_idx];
                        let len = dl.dir.length();
                        if len < 1e-6 {
                            continue;
                        }
                        let norm_dir = dl.dir / len;
                        let extent = 32768.0;
                        let p1 = dl.origin - norm_dir * extent;
                        let p2 = dl.origin + norm_dir * extent;
                        let sp1 = self.map_to_screen(p1, vc);
                        let sp2 = self.map_to_screen(p2, vc);
                        let alpha = (200 - (depth as u32 * 8).min(160)) as u8;
                        painter.line_segment(
                            [sp1, sp2],
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 200, 255, alpha)),
                        );
                    }
                }
            }
        }

        // Draw drag tool overlay
        self.draw_drag_overlay(painter, vc);
    }

    fn draw_visibility_layer(
        &self,
        painter: &egui::Painter,
        vc: Pos2,
        selected: usize,
        fill: Color32,
        is_visible: impl Fn(usize) -> bool,
    ) {
        let sel_color = Color32::from_rgba_unmultiplied(255, 60, 60, 80);
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if !is_visible(ss.index) {
                continue;
            }
            let color = if ss.index == selected {
                sel_color
            } else {
                fill
            };
            let points: Vec<Pos2> = ss
                .vertices
                .iter()
                .map(|&v| self.map_to_screen(v, vc))
                .collect();
            painter.add(filled_polygon(&points, color));
        }
    }

    fn draw_drag_overlay(&self, painter: &egui::Painter, vc: Pos2) {
        let pointer = painter.ctx().input(|i| i.pointer.interact_pos());
        let Some(cursor) = pointer else { return };
        let end_map = self.screen_to_map(cursor, vc);

        match self.state.drag_tool {
            DragTool::LineProbe {
                start,
            } => {
                let sp1 = self.map_to_screen(start, vc);
                painter.line_segment(
                    [sp1, cursor],
                    Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 100, 0, 200)),
                );
            }
            DragTool::RectSelect {
                start,
            } => {
                let lo = Vec2::new(start.x.min(end_map.x), start.y.min(end_map.y));
                let hi = Vec2::new(start.x.max(end_map.x), start.y.max(end_map.y));
                let tl = self.map_to_screen(Vec2::new(lo.x, hi.y), vc);
                let br = self.map_to_screen(Vec2::new(hi.x, lo.y), vc);
                let rect = egui::Rect::from_two_pos(tl, br);
                let stroke = Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 200, 0, 200));
                let fill = Color32::from_rgba_unmultiplied(255, 200, 0, 20);
                // Draw rect as 4 line segments + filled polygon
                painter.add(filled_polygon(
                    &[
                        Pos2::new(rect.min.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.max.y),
                    ],
                    fill,
                ));
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.min.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.max.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.max.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.max.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.min.y),
                    ],
                    stroke,
                );
            }
            DragTool::None => {}
        }
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading(&self.data.map_name);
        ui.label(format!("Sectors: {}", self.data.sectors.len()));
        ui.label(format!("Linedefs: {}", self.data.linedefs.len()));
        ui.label(format!("Subsectors: {}", self.data.subsectors.len()));
        ui.separator();

        ui.heading("Layers");
        ui.checkbox(&mut self.state.show_linedefs, "Linedefs");
        ui.checkbox(&mut self.state.show_sectors, "Sectors (textured)");
        ui.checkbox(&mut self.state.show_subsectors, "Subsector boundaries");
        ui.checkbox(&mut self.state.show_pvs, "PVS highlight");
        ui.checkbox(&mut self.state.show_mightsee, "MightSee overlay");
        ui.checkbox(&mut self.state.show_aabb, "Subsector AABBs");
        ui.checkbox(&mut self.state.show_divlines, "Divlines (selected)");
        ui.checkbox(&mut self.state.show_all_portals, "All portals");
        ui.checkbox(&mut self.state.show_pvs_portals, "PVS highlight portals");
        ui.checkbox(
            &mut self.state.show_map_polygon_edges,
            "Map polygon edges (red)",
        );
        ui.checkbox(&mut self.state.show_vertices, "Vertices");
        ui.separator();

        ui.heading("Debug Tools");
        ui.label("Cmd+drag: line probe");
        ui.label("Cmd+Shift+drag: rect select");
        ui.label("Output printed to stdout");
        ui.separator();

        ui.heading("PVS");
        let has_pvs = self.data.pvs_any.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(has_pvs, egui::Button::new("Save")).clicked() {
                self.save_pvs();
            }
            if ui.button("Rebuild").clicked() {
                self.rebuild_pvs();
            }
            if ui.button("Load Cache").clicked() {
                self.load_pvs_cache();
            }
        });
        if let Some(msg) = &self.state.status_msg {
            ui.label(msg.as_str());
        }
        ui.separator();

        // Selected subsector panel pinned to bottom of sidebar
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            let ss_count = self.data.subsectors.len();
            if let Some(sel) = self.state.selected_subsector {
                if self.state.pinned {
                    if ui.button("Unpin selection").clicked() {
                        self.state.pinned = false;
                    }
                }
                Self::draw_subsector_info_static(&self.data, ui, sel, ss_count);
            } else {
                ui.label("Hover a subsector to select");
            }
            let label = if self.state.pinned {
                "Selected (pinned)"
            } else {
                "Selected"
            };
            ui.colored_label(Color32::from_rgb(255, 160, 40), label);
            ui.separator();
        });
    }

    /// Draw hover info overlay at the bottom of the map viewport.
    /// Each line is a series of (color, text) spans for mixed-color rendering.
    fn draw_hover_overlay(&self, painter: &egui::Painter, viewport_rect: egui::Rect) {
        let ss_count = self.data.subsectors.len();
        let val = Color32::from_gray(210); // light grey for data values

        // Each line is a Vec of (color, text) spans
        let mut lines: Vec<Vec<(Color32, String)>> = Vec::new();

        // Fixed-width columns so SS and LD fields align vertically.
        // col1: type+id (6), col2: label+val (10), col3-5: label+val (10 each)
        // then spc/tag/pvs/ms columns.

        if let Some(hovered) = self.state.hovered_subsector {
            lines.push(Self::build_ss_spans(&self.data, hovered, ss_count));
        }

        if let Some(lid) = self.state.hovered_linedef {
            if let Some(ld) = self.data.linedefs.get(lid) {
                let lbl = Color32::from_rgb(255, 220, 100);
                let back: String = ld.back_sector_id.map_or("none".into(), |b| b.to_string());
                let spans = vec![
                    (lbl, "LD:".into()),
                    (val, format!("{:<5}", lid)),
                    (lbl, " front:".into()),
                    (val, format!("{:<5}", ld.front_sector_id)),
                    (lbl, " back:".into()),
                    (val, format!("{:<8}", back)),
                    (lbl, " sided:".into()),
                    (
                        val,
                        format!("{:<6}", if ld.is_two_sided { "2" } else { "1" }),
                    ),
                    (lbl, " spc:".into()),
                    (val, format!("{:<5}", ld.special)),
                    (lbl, " tag:".into()),
                    (val, format!("{:<5}", ld.tag)),
                ];
                lines.push(spans);
            }
        }

        if let Some(vid) = self.state.hovered_vertex {
            if let Some(vx) = self.data.vertices.get(vid) {
                let lbl = Color32::from_rgb(180, 255, 180);
                let ld_list: String = vx
                    .linedef_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let spans = vec![
                    (lbl, "VX:".into()),
                    (val, format!("{:<5}", vid)),
                    (lbl, " pos:".into()),
                    (val, format!("({:.1},{:.1})", vx.pos.x, vx.pos.y)),
                    (lbl, "  ld:".into()),
                    (
                        val,
                        if ld_list.is_empty() {
                            "none".into()
                        } else {
                            ld_list
                        },
                    ),
                ];
                lines.push(spans);
            }
        }

        if lines.is_empty() {
            return;
        }

        let font = egui::FontId::monospace(13.0);
        let line_height = 18.0;
        let padding = 6.0;
        let total_height = lines.len() as f32 * line_height + padding * 2.0;

        let bg_rect = egui::Rect::from_min_max(
            Pos2::new(viewport_rect.min.x, viewport_rect.max.y - total_height),
            viewport_rect.max,
        );
        painter.rect_filled(bg_rect, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 210));

        for (i, spans) in lines.iter().enumerate() {
            let y = bg_rect.min.y + padding + i as f32 * line_height;
            let mut x = bg_rect.min.x + padding;
            for (color, text) in spans {
                let galley = painter.layout_no_wrap(text.clone(), font.clone(), *color);
                let w = galley.rect.width();
                painter.galley(Pos2::new(x, y), galley, *color);
                x += w;
            }
        }
    }

    fn build_ss_spans(data: &ViewerData, ss_id: usize, ss_count: usize) -> Vec<(Color32, String)> {
        let lbl = Color32::from_rgb(100, 200, 255);
        let val = Color32::from_gray(210);
        let mut spans = Vec::new();

        if let Some(ss) = data.subsectors.get(ss_id) {
            let sid = ss.sector_id;
            spans.push((lbl, "SS:".into()));
            spans.push((val, format!("{:<5}", ss_id)));
            spans.push((lbl, " sector:".into()));
            spans.push((val, format!("{:<5}", sid)));
            if let Some(s) = data.sectors.get(sid) {
                spans.push((lbl, " floor:".into()));
                spans.push((val, format!("{:<7}", s.floor_height)));
                spans.push((lbl, " ceil:".into()));
                spans.push((val, format!("{:<7}", s.ceiling_height)));
                spans.push((lbl, " light:".into()));
                spans.push((val, format!("{:<5}", s.light_level)));
                spans.push((lbl, " spc:".into()));
                spans.push((val, format!("{:<5}", s.special)));
                spans.push((lbl, " tag:".into()));
                spans.push((val, format!("{:<5}", s.tag)));
            }
            if let Some(pvs) = &data.pvs {
                let vis = pvs.get_visible_subsectors(ss_id).len();
                spans.push((lbl, " pvs:".into()));
                spans.push((val, format!("{vis}/{ss_count}")));
            }
        }
        spans
    }

    fn draw_subsector_info_static(
        data: &ViewerData,
        ui: &mut egui::Ui,
        ss_id: usize,
        ss_count: usize,
    ) {
        if let Some(ss) = data.subsectors.get(ss_id) {
            let sid = ss.sector_id;
            // w = label column width for alignment
            const W: usize = 12;
            ui.monospace(format!("{:<W$}{}", "Subsector:", ss_id));
            ui.monospace(format!("{:<W$}{}", "Sector:", sid));
            if let Some(s) = data.sectors.get(sid) {
                ui.monospace(format!("{:<W$}{}", "Floor:", s.floor_height));
                ui.monospace(format!("{:<W$}{}", "Ceil:", s.ceiling_height));
                ui.monospace(format!("{:<W$}{}", "Light:", s.light_level));
                ui.monospace(format!("{:<W$}{}", "Special:", s.special));
                ui.monospace(format!("{:<W$}{}", "Tag:", s.tag));
            }
            if let Some(pvs) = &data.pvs {
                let vis = pvs.get_visible_subsectors(ss_id).len();
                ui.monospace(format!("{:<W$}{vis}/{ss_count}", "PVS:"));
            }
        }
    }
}

impl eframe::App for MapViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.state.zoom == 1.0 && self.state.offset == EVec2::ZERO {
            let available = ctx.available_rect();
            let viewport_size = EVec2::new(available.width() - 210.0, available.height());
            self.fit_zoom(viewport_size);
        }

        egui::SidePanel::left("sidebar")
            .default_width(200.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_sidebar(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(20)))
            .show(ctx, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let vc = response.rect.center();

                self.update_hover(vc, response.hover_pos());
                self.handle_input(&response);
                self.draw_layers(&painter, vc);
                self.draw_hover_overlay(&painter, response.rect);
            });
    }
}

pub fn run(data: ViewerData) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title(format!("pvs-tool - {}", data.map_name)),
        ..Default::default()
    };
    eframe::run_native(
        "pvs-tool viewer",
        options,
        Box::new(|_cc| Ok(Box::new(MapViewerApp::new(data)))),
    )
    .unwrap();
}

// ── Helpers ──

/// Validate a subsector polygon at extraction time, logging detail for any
/// issues.
fn validate_polygon(subsector_id: usize, sector_id: usize, verts: &[Vec2]) {
    let n = verts.len();

    for (i, v) in verts.iter().enumerate() {
        if !v.x.is_finite() || !v.y.is_finite() {
            log::warn!(
                "ss{subsector_id} (sector {sector_id}): vertex {i}/{n} non-finite ({}, {})",
                v.x,
                v.y
            );
            return;
        }
    }

    // Signed area via shoelace
    let mut area = 0.0_f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += verts[i].x * verts[j].y;
        area -= verts[j].x * verts[i].y;
    }
    area *= 0.5;

    if area.abs() < 0.5 {
        let mut dupes = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if (verts[i] - verts[j]).length() < 0.01 {
                    dupes.push((i, j));
                }
            }
        }

        let seg_info: Vec<String> = verts
            .iter()
            .map(|v| format!("({:.1},{:.1})", v.x, v.y))
            .collect();
        log::warn!(
            "ss{subsector_id} (sector {sector_id}): degenerate polygon, {n} verts, area={area:.4}, \
             verts=[{}]{}",
            seg_info.join(", "),
            if dupes.is_empty() {
                String::new()
            } else {
                format!(", duplicate pairs: {:?}", dupes)
            }
        );
    }
}

/// Create a filled polygon shape using centroid fan triangulation.
fn filled_polygon(points: &[Pos2], color: Color32) -> egui::Shape {
    if points.len() < 3 {
        return egui::Shape::Noop;
    }
    let n = points.len();
    let centroid = Pos2::new(
        points.iter().map(|p| p.x).sum::<f32>() / n as f32,
        points.iter().map(|p| p.y).sum::<f32>() / n as f32,
    );
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centroid, color);
    for p in points {
        mesh.colored_vertex(*p, color);
    }
    let n = n as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    egui::Shape::Mesh(Arc::new(mesh))
}

fn point_to_segment_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.dot(ab);
    if len_sq < 1e-12 {
        return (p - a).length();
    }
    let t = (p - a).dot(ab) / len_sq;
    (p - (a + ab * t.clamp(0.0, 1.0))).length()
}

fn point_in_polygon(p: Vec2, verts: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = verts.len() - 1;
    for i in 0..verts.len() {
        let vi = verts[i];
        let vj = verts[j];
        if ((vi.y > p.y) != (vj.y > p.y))
            && (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ── Geometry helpers for debug queries ──

fn point_in_rect(p: Vec2, lo: Vec2, hi: Vec2) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

/// True if any vertex of the polygon is inside the rect.
fn polygon_inside_rect(verts: &[Vec2], lo: Vec2, hi: Vec2) -> bool {
    verts.iter().any(|&v| point_in_rect(v, lo, hi))
}

/// Test if two line segments intersect (a1-a2 and b1-b2).
fn segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let d1 = a2 - a1;
    let d2 = b2 - b1;
    let cross = d1.x * d2.y - d1.y * d2.x;
    if cross.abs() < 1e-10 {
        return false; // parallel
    }
    let d = b1 - a1;
    let t = (d.x * d2.y - d.y * d2.x) / cross;
    let u = (d.x * d1.y - d.y * d1.x) / cross;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// Test if a line segment intersects any edge of a polygon.
fn line_intersects_polygon(a: Vec2, b: Vec2, verts: &[Vec2]) -> bool {
    // Check if line endpoints are inside polygon
    if point_in_polygon(a, verts) || point_in_polygon(b, verts) {
        return true;
    }
    // Check if line intersects any polygon edge
    let n = verts.len();
    for i in 0..n {
        if segments_intersect(a, b, verts[i], verts[(i + 1) % n]) {
            return true;
        }
    }
    false
}
