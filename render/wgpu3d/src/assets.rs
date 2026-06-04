//! Bake Doom wall/flat textures into RGBA atlas array layers for the GPU.
//!
//! Each texture's palette-indexed texels are resolved to RGBA via the base
//! (unlit, tint-0) palette and shelf-packed into a `texture_2d_array`: shelves
//! grow downward until a layer reaches `layer_height`, then spill to the next
//! layer. A per-id rect (origin + size + layer) lets the shader wrap within a
//! texture's region and sample the right layer. `u16::MAX` texels are transparent.

use pic_data::PicData;

/// Atlas shelf width. Widened only if a single texture is wider. Doom textures
/// are small, so this packs a full IWAD+PWAD across few layers.
const ATLAS_WIDTH: u32 = 2048;
/// Transparent palette sentinel in source texel data.
const TRANSPARENT: u16 = u16::MAX;

/// One texture's placement in the atlas: pixel origin + source dimensions + the
/// array layer it lives in. Packed for a storage buffer (std430): 6 u32 padded
/// to 8 (32 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtlasRect {
    pub origin: [u32; 2],
    pub size: [u32; 2],
    pub layer: u32,
    pub _pad: [u32; 3],
}

/// A baked atlas array: RGBA pixels (layer-major, each layer `width*height`),
/// the layer count, and the per-texture-id rect table.
pub struct Atlas {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub rects: Vec<AtlasRect>,
}

impl Atlas {
    /// Pack every wall texture. `max_dim` is the device 2D-texture limit (the
    /// per-layer height cap).
    pub fn walls(pic_data: &PicData, max_dim: u32) -> Self {
        let sizes: Vec<(u32, u32)> = (0..pic_data.num_textures())
            .map(|i| {
                let t = pic_data.get_texture(i);
                (t.width as u32, t.height as u32)
            })
            .collect();
        Self::pack(&sizes, max_dim, |id, dst, stride, ox, oy| {
            let t = pic_data.get_texture(id);
            blit(&t.data, t.width, t.height, pic_data, dst, stride, ox, oy);
        })
    }

    /// Pack every flat (all 64×64). `max_dim` is the per-layer height cap.
    pub fn flats(pic_data: &PicData, max_dim: u32) -> Self {
        let sizes: Vec<(u32, u32)> = (0..pic_data.num_flats())
            .map(|i| {
                let f = pic_data.get_flat(i);
                (f.width as u32, f.height as u32)
            })
            .collect();
        Self::pack(&sizes, max_dim, |id, dst, stride, ox, oy| {
            let f = pic_data.get_flat(id);
            blit(&f.data, f.width, f.height, pic_data, dst, stride, ox, oy);
        })
    }

    /// Pack every sprite patch into an atlas array, returning the atlas plus a
    /// per-patch metadata table (pivot offsets) indexed by patch id. Sprite
    /// pixels are `Vec<Vec<u16>>` (column-major posts), not the flat `&[u16]`
    /// walls/flats use, so this blits via [`blit_sprite`].
    pub fn sprites(pic_data: &PicData, max_dim: u32) -> SpriteAtlas {
        let count = pic_data.num_sprite_patches();
        let sizes: Vec<(u32, u32)> = (0..count)
            .map(|i| {
                let p = pic_data.sprite_patch(i);
                let w = p.data.len() as u32;
                let h = p.data.first().map_or(0, |c| c.len()) as u32;
                (w, h)
            })
            .collect();
        let meta = (0..count)
            .map(|i| SpriteMeta {
                left_offset: pic_data.sprite_patch(i).left_offset,
            })
            .collect();
        let atlas = Self::pack(&sizes, max_dim, |id, dst, stride, ox, oy| {
            let p = pic_data.sprite_patch(id);
            blit_sprite(&p.data, pic_data, dst, stride, ox, oy);
        });
        SpriteAtlas {
            atlas,
            meta,
        }
    }

    /// Shelf-pack `sizes` into array layers. Width is `ATLAS_WIDTH` or the widest
    /// texture; shelves grow down until the next shelf would exceed `max_dim`,
    /// then spill to a new layer. All layers share the same dimensions (array
    /// requirement); `height` is the tallest layer's fill, capped at `max_dim`.
    fn pack(
        sizes: &[(u32, u32)],
        max_dim: u32,
        mut blit_one: impl FnMut(usize, &mut [u8], u32, u32, u32),
    ) -> Self {
        let width = sizes
            .iter()
            .map(|&(w, _)| w)
            .max()
            .unwrap_or(1)
            .max(ATLAS_WIDTH)
            .min(max_dim);

        // Pass 1: lay out shelves across layers, recording each rect.
        let mut rects = vec![
            AtlasRect {
                origin: [0, 0],
                size: [0, 0],
                layer: 0,
                _pad: [0; 3],
            };
            sizes.len()
        ];
        let (mut x, mut y, mut shelf_h, mut layer, mut max_y) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for (id, &(w, h)) in sizes.iter().enumerate() {
            if w == 0 || h == 0 {
                continue;
            }
            if x + w > width {
                // New shelf; spill to a new layer if it would overflow this one.
                x = 0;
                y += shelf_h;
                shelf_h = 0;
                if y + h > max_dim {
                    layer += 1;
                    y = 0;
                }
            }
            rects[id] = AtlasRect {
                origin: [x, y],
                size: [w, h],
                layer,
                _pad: [0; 3],
            };
            x += w;
            shelf_h = shelf_h.max(h);
            max_y = max_y.max(y + shelf_h);
        }
        let layers = layer + 1;
        let height = max_y.max(1).min(max_dim);

        // Pass 2: allocate the layer-major buffer and blit each texture. The
        // blit uses dst row = layer*height + origin.y so layers are contiguous.
        let layer_px = (width * height) as usize;
        let mut pixels = vec![0u8; layer_px * 4 * layers as usize];
        for (id, &(w, h)) in sizes.iter().enumerate() {
            if w == 0 || h == 0 {
                continue;
            }
            let r = rects[id];
            let dst_oy = r.layer * height + r.origin[1];
            blit_one(id, &mut pixels, width, r.origin[0], dst_oy);
        }
        Self {
            pixels,
            width,
            height,
            layers,
            rects,
        }
    }
}

/// Per-sprite-patch pivot offset (texels), indexed by patch id. The renderer
/// uses `left_offset` for the billboard horizontal anchor; width/height come
/// from the atlas rect's `size`.
#[derive(Clone, Copy)]
pub struct SpriteMeta {
    pub left_offset: i32,
}

/// A baked sprite atlas: the packed RGBA array + per-patch pivot table.
pub struct SpriteAtlas {
    pub atlas: Atlas,
    pub meta: Vec<SpriteMeta>,
}

/// Resolve column-major palette-indexed `src` (w×h) to RGBA and write it into the
/// atlas `dst` (row-major, `stride` px wide) at pixel `(ox, oy)`.
fn blit(
    src: &[u16],
    w: usize,
    h: usize,
    pic_data: &PicData,
    dst: &mut [u8],
    stride: u32,
    ox: u32,
    oy: u32,
) {
    let palette = &pic_data.palettes()[0];
    for col in 0..w {
        for row in 0..h {
            let texel = src[col * h + row];
            let dst_x = ox as usize + col;
            let dst_y = oy as usize + row;
            let di = (dst_y * stride as usize + dst_x) * 4;
            if texel == TRANSPARENT {
                dst[di + 3] = 0;
                continue;
            }
            let argb = palette.0[texel as usize];
            dst[di] = (argb >> 16) as u8;
            dst[di + 1] = (argb >> 8) as u8;
            dst[di + 2] = argb as u8;
            dst[di + 3] = 255;
        }
    }
}

/// Resolve a sprite patch's column-major `Vec<Vec<u16>>` posts to RGBA into the
/// atlas `dst` at `(ox, oy)`. Inner columns may be short/empty (Doom posts);
/// only present rows are written, the rest stay transparent (zeroed buffer).
fn blit_sprite(
    data: &[Vec<u16>],
    pic_data: &PicData,
    dst: &mut [u8],
    stride: u32,
    ox: u32,
    oy: u32,
) {
    let palette = &pic_data.palettes()[0];
    for (col, column) in data.iter().enumerate() {
        for (row, &texel) in column.iter().enumerate() {
            if texel == TRANSPARENT {
                continue;
            }
            let dst_x = ox as usize + col;
            let dst_y = oy as usize + row;
            let di = (dst_y * stride as usize + dst_x) * 4;
            let argb = palette.0[texel as usize];
            dst[di] = (argb >> 16) as u8;
            dst[di + 1] = (argb >> 8) as u8;
            dst[di + 2] = argb as u8;
            dst[di + 3] = 255;
        }
    }
}
