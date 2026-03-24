/// KVX voxel model parser.
///
/// Format: numbytes(u32), xsiz/ysiz/zsiz(u32x3), xpivot/ypivot/zpivot(i32x3 8.8
/// fixed), xoffset[xsiz+1](u32), xyoffset[xsiz*(ysiz+1)](u16), then slab data
/// per (x,y) column.

pub struct VoxelModel {
    pub xsiz: u32,
    pub ysiz: u32,
    pub zsiz: u32,
    pub xpivot: f32,
    pub ypivot: f32,
    pub zpivot: f32,
    pub grid: Vec<u8>,
    /// Optional embedded palette (256 entries × 3 bytes, 6-bit per channel
    /// 0-63)
    pub palette: Option<Vec<u8>>,
}

impl VoxelModel {
    /// Remap grid colour indices from KVX palette to Doom palette.
    /// KVX palette is 6-bit per channel (0-63). Each index is matched
    /// to the closest Doom palette entry.
    pub fn remap_to_doom_palette(&mut self, doom_palette: &[u8]) {
        let kvx_pal = match &self.palette {
            Some(p) => p,
            None => return, // no embedded palette, indices are already Doom palette
        };
        if doom_palette.len() < 768 || kvx_pal.len() < 768 {
            return;
        }

        // Build remap table: for each KVX index, find closest Doom palette entry
        let mut remap = [0u8; 256];
        for i in 0..256 {
            // Convert 6-bit to 8-bit
            let kr = (kvx_pal[i * 3] << 2) | (kvx_pal[i * 3] >> 4);
            let kg = (kvx_pal[i * 3 + 1] << 2) | (kvx_pal[i * 3 + 1] >> 4);
            let kb = (kvx_pal[i * 3 + 2] << 2) | (kvx_pal[i * 3 + 2] >> 4);

            // Find closest Doom palette entry (skip index 255 = transparent)
            let mut best_dist = u32::MAX;
            let mut best_idx = 0u8;
            for j in 0..256 {
                let dr = kr as i32 - doom_palette[j * 3] as i32;
                let dg = kg as i32 - doom_palette[j * 3 + 1] as i32;
                let db = kb as i32 - doom_palette[j * 3 + 2] as i32;
                let dist = (dr * dr + dg * dg + db * db) as u32;
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = j as u8;
                }
            }
            remap[i] = best_idx;
        }

        // Apply remap to all grid values
        for v in &mut self.grid {
            if *v != 255 {
                *v = remap[*v as usize];
            }
        }
    }

    pub fn get(&self, x: u32, y: u32, z: u32) -> u8 {
        if x >= self.xsiz || y >= self.ysiz || z >= self.zsiz {
            return 255;
        }
        self.grid[(x * self.ysiz * self.zsiz + y * self.zsiz + z) as usize]
    }

    pub fn load(data: &[u8]) -> Result<Self, String> {
        if data.len() < 28 {
            return Err("KVX data too short for header".into());
        }

        let r32 = |off: usize| u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let ri32 = |off: usize| i32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let r16 = |off: usize| u16::from_le_bytes(data[off..off + 2].try_into().unwrap());

        let _numbytes = r32(0);
        let xsiz = r32(4);
        let ysiz = r32(8);
        let zsiz = r32(12);
        let xpivot = ri32(16) as f32 / 256.0;
        let ypivot = ri32(20) as f32 / 256.0;
        let zpivot = ri32(24) as f32 / 256.0;

        if xsiz == 0 || ysiz == 0 || zsiz == 0 {
            return Err("KVX dimensions are zero".into());
        }
        if xsiz > 1024 || ysiz > 1024 || zsiz > 1024 {
            return Err(format!(
                "KVX dimensions too large: {}x{}x{}",
                xsiz, ysiz, zsiz
            ));
        }

        let hdr = 28;
        let xoff_len = (xsiz + 1) as usize * 4;
        let xyoff_len = (xsiz * (ysiz + 1)) as usize * 2;
        let offsets_end = hdr + xoff_len + xyoff_len;

        if data.len() < offsets_end {
            return Err("KVX data too short for offset tables".into());
        }

        let xoff_base = hdr;
        let xyoff_base = hdr + xoff_len;

        let total = (xsiz * ysiz * zsiz) as usize;
        let mut grid = vec![255u8; total];

        // Slab (voxel) data starts after all offset tables
        let voxdata_start = offsets_end;
        // xoffset[0] is the base for relative addressing
        let xoff0 = r32(xoff_base) as usize;

        for x in 0..xsiz {
            let xoff = r32(xoff_base + x as usize * 4) as usize;
            let x_base = voxdata_start + (xoff - xoff0);
            for y in 0..ysiz {
                let xyoff_idx = (x * (ysiz + 1) + y) as usize;
                let col_start = r16(xyoff_base + xyoff_idx * 2) as usize;
                let col_end = r16(xyoff_base + (xyoff_idx + 1) * 2) as usize;

                let abs_start = x_base + col_start;
                let abs_end = x_base + col_end;

                if abs_end > data.len() {
                    return Err(format!(
                        "KVX slab data out of bounds at x={} y={}: {}..{} > {}",
                        x,
                        y,
                        abs_start,
                        abs_end,
                        data.len()
                    ));
                }

                let mut pos = abs_start;
                while pos < abs_end {
                    if pos + 3 > abs_end {
                        break;
                    }
                    let ztop = data[pos] as u32;
                    let zleng = data[pos + 1] as u32;
                    let _vis = data[pos + 2];
                    pos += 3;

                    if pos + zleng as usize > abs_end {
                        break;
                    }

                    for i in 0..zleng {
                        let z = ztop + i;
                        if z < zsiz {
                            let idx = (x * ysiz * zsiz + y * zsiz + z) as usize;
                            grid[idx] = data[pos + i as usize];
                        }
                    }
                    pos += zleng as usize;
                }
            }
        }

        // Trailing 768-byte palette (last 768 bytes of file, 6-bit per channel)
        let palette = if data.len() >= 768 {
            let pal_start = data.len() - 768;
            Some(data[pal_start..].to_vec())
        } else {
            None
        };

        Ok(VoxelModel {
            xsiz,
            ysiz,
            zsiz,
            xpivot,
            ypivot,
            zpivot,
            grid,
            palette,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ammoa() {
        let data = std::fs::read(test_utils::kvx_path("ammoa.kvx")).expect("ammoa.kvx not found");
        let model = VoxelModel::load(&data).expect("failed to parse ammoa.kvx");
        assert_eq!(model.xsiz, 30, "xsiz");
        assert_eq!(model.ysiz, 16, "ysiz");
        assert_eq!(model.zsiz, 14, "zsiz");

        // Grid should not be all empty
        let non_empty = model.grid.iter().filter(|&&v| v != 255).count();
        assert!(non_empty > 0, "grid is entirely empty");
        log::info!(
            "ammoa: {}x{}x{}, {} occupied voxels",
            model.xsiz,
            model.ysiz,
            model.zsiz,
            non_empty
        );
    }
}
