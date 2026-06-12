//! Whole-map PNG export via the GPU pipeline.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

use crate::level_editor::map_bounds;
use crate::render::export_camera;
use crate::render::view::WorldRect;
use crate::state::SharedState;

pub const PNG_MARGIN_UNITS: f32 = 64.0;
pub const PNG_MAX_DIM: u32 = 16384;
pub const PNG_SCALE_PRESETS: &[f32] = &[0.25, 0.5, 1.0, 2.0, 4.0];

#[derive(Debug)]
pub enum PngExportError {
    EmptyMap,
    TooLarge {
        width: u32,
        height: u32,
    },
    /// No canvas frame rendered yet.
    DeviceNotReady,
    Encode(png::EncodingError),
    Io(io::Error),
}

impl fmt::Display for PngExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMap => write!(f, "map has no geometry to render"),
            Self::TooLarge {
                width,
                height,
            } => {
                write!(f, "{width}x{height} exceeds the {PNG_MAX_DIM}px limit")
            }
            Self::DeviceNotReady => write!(f, "gpu device not ready"),
            Self::Encode(e) => write!(f, "png encode error: {e}"),
            Self::Io(e) => write!(f, "png io error: {e}"),
        }
    }
}

impl Error for PngExportError {}

impl From<io::Error> for PngExportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<png::EncodingError> for PngExportError {
    fn from(e: png::EncodingError) -> Self {
        Self::Encode(e)
    }
}

/// Map image dimensions at `scale`, including margin.
pub fn image_size(bounds: WorldRect, scale: f32) -> (u32, u32) {
    let w = ((bounds.max_x - bounds.min_x + 2.0 * PNG_MARGIN_UNITS) * scale).ceil() as u32;
    let h = ((bounds.max_y - bounds.min_y + 2.0 * PNG_MARGIN_UNITS) * scale).ceil() as u32;
    (w.max(1), h.max(1))
}

/// Renders the map at `scale` via the GPU pipeline and writes a PNG to `path`.
pub fn export_png(state: &SharedState, scale: f32, path: &Path) -> Result<(), PngExportError> {
    let map = state.app.map.as_ref().ok_or(PngExportError::EmptyMap)?;
    let bounds = map_bounds(map).ok_or(PngExportError::EmptyMap)?;
    let (width, height) = image_size(bounds, scale);
    if width > PNG_MAX_DIM || height > PNG_MAX_DIM {
        return Err(PngExportError::TooLarge {
            width,
            height,
        });
    }

    let centre = [
        bounds.min_x.midpoint(bounds.max_x),
        bounds.min_y.midpoint(bounds.max_y),
    ];
    let camera = export_camera(centre, scale, width as f32, height as f32);
    let rgba = state
        .wgpu
        .render_canvas_rgba(camera, width, height)
        .ok_or(PngExportError::DeviceNotReady)?;

    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::EditorAssets;
    use crate::render::atlas;
    use crate::render::editor_camera::CameraMode;
    use crate::render::export_camera;
    use crate::render::frame::{self, FrameInput};
    use crate::render::input::{SectorFill, Selection};
    use crate::render::sprites::ThingSpriteCache;
    use crate::render::style::CanvasStyle;
    use crate::render::triangulate::build_sector_tris;
    use crate::render::wgpu::headless_renderer;
    use std::collections::HashMap;
    use std::{env, fs};

    #[test]
    fn image_size_includes_margin_and_scales() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let bounds = map_bounds(&map).expect("non-empty");
        let (w1, h1) = image_size(bounds, 1.0);
        let (w2, h2) = image_size(bounds, 2.0);
        assert!(w2 >= w1 * 2 - 1 && w2 <= w1 * 2 + 1);
        assert!(h2 >= h1 * 2 - 1 && h2 <= h1 * 2 + 1);
        let span = (bounds.max_x - bounds.min_x).ceil() as u32;
        assert!(w1 > span);
    }

    #[test]
    fn export_renders_decodable_nonblank_png() {
        let Some(mut r) = headless_renderer() else {
            eprintln!("no wgpu adapter; skipping PNG export render test");
            return;
        };
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let names = atlas::collect_wall_names(&assets, &map);
        assets.ensure_composed(&names, &wad);
        let tris = build_sector_tris(&map);
        let (data, maps) = atlas::build(&assets, &map, &ThingSpriteCache::default(), 1);
        r.set_atlases(&data);

        let scale = 0.25;
        let bounds = map_bounds(&map).expect("non-empty");
        let (w, h) = image_size(bounds, scale);
        let centre = [
            bounds.min_x.midpoint(bounds.max_x),
            bounds.min_y.midpoint(bounds.max_y),
        ];
        let camera = export_camera(centre, scale, w as f32, h as f32);
        let style = CanvasStyle::default();
        let sel = Selection::default();
        let (extents, colors) = (HashMap::new(), HashMap::new());
        let input = FrameInput {
            map: &map,
            tris: &tris,
            zoom: scale,
            pixel_ratio: 1.0,
            style: &style,
            selection: &sel,
            grid: 0,
            fill: SectorFill::Texture,
            selected_sectors: &[],
            thing_visible: &|_| true,
            thing_extents: &extents,
            thing_colors: &colors,
            atlas: &maps,
            thing_radius: &|_| 20.0,
            sector_gradient: colorous::PLASMA,
            highlight_unenclosed: false,
            mode: CameraMode::TopDown,
            grid_z: 0.0,
            vert_z: &[],
        };
        let f = frame::build_map_geometry(&input);
        let grid = frame::grid_style(&input);
        let brightness: Vec<f32> = map
            .sectors
            .iter()
            .map(|s| s.light_level.clamp(0, 255) as f32 / 255.0)
            .collect();
        r.set_sector_data(&brightness, &f.sector_attrs, &f.sector3d);
        let rgba = r.render_frame_rgba(&f, grid, camera, w, h);
        assert_eq!(rgba.len(), (w * h * 4) as usize);

        let path = env::temp_dir().join("editor_wgpu_png_test.png");
        {
            let file = File::create(&path).expect("create");
            let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().expect("header");
            wr.write_image_data(&rgba).expect("write");
            wr.finish().expect("finish");
        }
        let decoder = png::Decoder::new(File::open(&path).expect("open"));
        let reader = decoder.read_info().expect("valid png");
        assert_eq!((reader.info().width, reader.info().height), (w, h));
        fs::remove_file(&path).ok();

        let non_bg = rgba
            .chunks_exact(4)
            .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
            .count();
        assert!(non_bg > (w * h / 20) as usize, "exported map is not blank");
    }
}
