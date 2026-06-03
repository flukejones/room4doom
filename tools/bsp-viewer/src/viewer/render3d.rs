use egui::{ColorImage, TextureHandle, TextureOptions};
use glam::Vec3;
use level::LevelData;
use math::{Angle, Bam, FixedT};
use pic_data::PicData;
use render_common::{BufferSize, DrawBuffer, RenderPspDef, RenderView};
use software3d::{DebugColourMode, DebugDrawOptions, Software3D};
use wad::WadData;

pub const FOV: f32 = std::f32::consts::FRAC_PI_2;
const CLEAR_COLOUR: u32 = 0xFF000000;

/// 3D render mode selected from the UI.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Render3DMode {
    Textured,
    SolidSectors,
    Wireframe,
}

/// Free-fly camera.
pub struct Camera3D {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

impl Camera3D {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
        )
    }

    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), -self.yaw.cos(), 0.0)
    }
}

struct FrameBuffer {
    size: BufferSize,
    data: Vec<u32>,
    w: usize,
}

impl FrameBuffer {
    fn new(w: usize, h: usize) -> Self {
        Self {
            size: BufferSize::new(w, h),
            data: vec![0u32; w * h],
            w,
        }
    }
}

impl DrawBuffer for FrameBuffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        self.data[y * self.w + x] = colour;
    }
    fn read_pixel(&self, x: usize, y: usize) -> u32 {
        self.data[y * self.w + x]
    }
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }
    fn pitch(&self) -> usize {
        self.w
    }
    fn buf_mut(&mut self) -> &mut [u32] {
        &mut self.data
    }
    fn debug_flip_and_present(&mut self) {}
}

pub struct Renderer3D {
    sw: Software3D,
    pics: PicData,
    fb: FrameBuffer,
    mode: Render3DMode,
    w: usize,
    h: usize,
}

impl Renderer3D {
    pub fn new(wad: &WadData, mode: Render3DMode, w: usize, h: usize) -> Self {
        // The viewer never draws sprites, but PicData requires a non-empty
        // sprite list; one placeholder name suffices.
        let pics = PicData::init(wad, &["TROO"]);
        Self {
            sw: Software3D::new(w as f32, h as f32, FOV, debug_opts(mode)),
            pics,
            fb: FrameBuffer::new(w, h),
            mode,
            w,
            h,
        }
    }

    /// Render the level from `cam` into an egui texture, recreating the
    /// renderer when the mode or viewport size changes.
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        level: &LevelData,
        cam: &Camera3D,
        mode: Render3DMode,
        size: (usize, usize),
    ) -> TextureHandle {
        if self.mode != mode {
            self.mode = mode;
            self.sw = Software3D::new(self.w as f32, self.h as f32, FOV, debug_opts(mode));
        }
        if (self.w, self.h) != size {
            self.w = size.0;
            self.h = size.1;
            self.fb = FrameBuffer::new(self.w, self.h);
            self.sw.resize(self.w as f32, self.h as f32);
        }

        self.render_to_buffer(level, cam);

        let pixels: Vec<egui::Color32> = self
            .fb
            .data
            .iter()
            .map(|&c| egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8))
            .collect();
        let image = ColorImage {
            size: [self.w, self.h],
            pixels,
        };
        ctx.load_texture("bsp3d", image, TextureOptions::NEAREST)
    }

    /// Render one frame into the framebuffer (no egui dependency).
    fn render_to_buffer(&mut self, level: &LevelData, cam: &Camera3D) {
        let view = render_view(cam);
        self.fb.data.fill(CLEAR_COLOUR);
        self.sw
            .draw_view(&view, level, &mut self.pics, &mut self.fb);
    }
}

fn debug_opts(mode: Render3DMode) -> DebugDrawOptions {
    let mut opts = DebugDrawOptions {
        clear_colour: Some(CLEAR_COLOUR),
        ..Default::default()
    };
    match mode {
        Render3DMode::Textured => {}
        Render3DMode::SolidSectors => opts.colour_mode = DebugColourMode::SectorId,
        Render3DMode::Wireframe => opts.wireframe = true,
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building a renderer and drawing one frame must not panic (exercises
    /// PicData init + the full BSP traverse + rasteriser) in every mode.
    #[test]
    fn render_one_frame_each_mode() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        let path = std::path::PathBuf::from(home).join("DOOM/doom.wad");
        if !path.exists() {
            eprintln!("skipping: {} not found", path.display());
            return;
        }
        let wad = WadData::new(&path);
        let mut level = Box::pin(LevelData::default());
        let pics = PicData::init(&wad, &["TROO"]);
        unsafe { level.as_mut().get_unchecked_mut() }.load(
            "E1M1",
            |n| pics.flat_num_for_name(n),
            &wad,
            None,
            None,
        );
        let cam = Camera3D {
            pos: Vec3::new(1000.0, -3000.0, 64.0),
            yaw: 0.0,
            pitch: 0.0,
        };
        for mode in [
            Render3DMode::Textured,
            Render3DMode::SolidSectors,
            Render3DMode::Wireframe,
        ] {
            let mut r = Renderer3D::new(&wad, mode, 320, 240);
            r.render_to_buffer(&level, &cam);
        }
    }
}

fn render_view(cam: &Camera3D) -> RenderView {
    let fp = FixedT::from_f32;
    RenderView {
        x: fp(cam.pos.x),
        y: fp(cam.pos.y),
        z: fp(cam.pos.z),
        viewz: fp(cam.pos.z),
        viewheight: fp(0.0),
        angle: Angle::<Bam>::new(cam.yaw),
        lookdir: cam.pitch,
        fixedcolormap: 0,
        extralight: 0,
        is_shadow: false,
        subsector_id: 0,
        psprites: [RenderPspDef::default(); 2],
        sector_lightlevel: 0,
        player_mobj_id: 0,
        frac: 1.0,
        frac_fp: fp(1.0),
        game_tic: 0,
    }
}
