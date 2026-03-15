//! Sky texture extension.
//!
//! Pipeline (symmetric around the original texture):
//!
//!  ┌────────────────────────────┐ top
//!  │  pure zenith colour        │
//!  ├────────────────────────────┤ fade_end
//!  │  jitter fades into zenith  │
//!  ├────────────────────────────┤ fade_start
//!  │  jitter zone (up ext)      │
//!  ├────────────────────────────┤ join top
//!  │  original texture          │
//!  ├────────────────────────────┤ join bottom
//!  │  jitter zone (down ext)    │
//!  ├────────────────────────────┤ fade_start
//!  │  jitter fades into nadir   │
//!  ├────────────────────────────┤ fade_end
//!  │  pure nadir colour         │
//!  └────────────────────────────┘ bottom
//!
//! Combined buffer layout (column-major):
//!   rows `0..height`                         — original texture
//!   rows `height..height+SKY_EXTEND_ROWS`    — upward extension
//!   rows `height+SKY_EXTEND_ROWS..total`     — downward extension

/// Sky texture tiles this many times around 360°.
pub(crate) const SKY_TILES: f32 = 4.0;
/// Vertical stretch factor: sky appears this much taller than the screen.
pub(crate) const SKY_V_STRETCH: f32 = 1.33;
/// Number of rows generated above the original texture.
pub(crate) const SKY_EXTEND_ROWS: usize = 256;
/// Number of rows generated below the original texture.
pub(crate) const SKY_DOWN_ROWS: usize = SKY_EXTEND_ROWS;

/// Rows from the texture top/bottom used as jitter source material.
const SKY_SOURCE_ROWS: usize = 6;
/// Rows from the texture top/bottom averaged to derive the zenith/nadir colour.
const SKY_ZENITH_ROWS: usize = 4;
/// Fraction of extension rows that are pure jitter before the fade begins.
const SKY_FADE_START_FRAC: f32 = 0.01;
/// Fraction of extension rows over which jitter dissolves into zenith/nadir.
const SKY_FADE_SPAN_FRAC: f32 = 0.22;
/// Rows into the original texture (from each join) that also receive jitter,
/// blended with full strength at the join and fading to zero at this depth.
const SKY_JITTER_START_ROW: usize = 24;
/// Extension rows per source row advanced by the jitter walk.
const SKY_JITTER_WALK_RATE: f32 = SKY_JITTER_START_ROW as f32 / 2.0;
/// Maximum per-column drift added to the walk position, in fractional source
/// rows.
const SKY_MAX_DRIFT: f32 = 1.8;
/// Probability per row of the drift stepping ±0.3.
const SKY_DRIFT_SMOOTHNESS: f32 = 0.08;

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hrand(x: u32, y: u32, seed: u32) -> f32 {
    let n = x
        .wrapping_add(y.wrapping_mul(2654435761))
        .wrapping_add(seed);
    let n = n ^ (n >> 16);
    let n = n.wrapping_mul(0x45D9F3B);
    let n = n ^ (n >> 16);
    (n & 0xFFFF) as f32 / 65536.0
}

#[inline]
fn resolve(
    data: &[usize],
    col: usize,
    row: usize,
    height: usize,
    colourmap: &[usize],
    palette: &[u32],
) -> u32 {
    let idx = data[col * height + row];
    if idx == usize::MAX {
        0
    } else {
        palette[colourmap[idx]]
    }
}

fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let ar = (a >> 16) as u8;
    let ag = (a >> 8) as u8;
    let ab = a as u8;
    let br = (b >> 16) as u8;
    let bg = (b >> 8) as u8;
    let bb = b as u8;
    let r = (ar as f32 + (br as f32 - ar as f32) * t) as u8;
    let g = (ag as f32 + (bg as f32 - ag as f32) * t) as u8;
    let b = (ab as f32 + (bb as f32 - ab as f32) * t) as u8;
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Sample the top source strip at `row_idx` rows from the join, with drift.
/// Walk advances from row 0 (join) toward row `source_rows-1` (away from join).
fn jitter_sample_up(
    data: &[usize],
    col: usize,
    row_idx: usize,
    drift: f32,
    source_rows: usize,
    height: usize,
    colourmap: &[usize],
    palette: &[u32],
) -> u32 {
    let walk = (row_idx as f32 / SKY_JITTER_WALK_RATE).min((source_rows - 1) as f32);
    let raw = (walk + drift).clamp(0.0, (source_rows - 1) as f32);
    let a = raw as usize;
    let b = (a + 1).min(source_rows - 1);
    lerp_color(
        resolve(data, col, a, height, colourmap, palette),
        resolve(data, col, b, height, colourmap, palette),
        raw.fract(),
    )
}

/// Sample the bottom source strip at `row_idx` rows from the join, with drift.
/// Walk advances from row `height-1` (join) toward row `height-source_rows`
/// (away).
fn jitter_sample_down(
    data: &[usize],
    col: usize,
    row_idx: usize,
    drift: f32,
    source_rows: usize,
    height: usize,
    colourmap: &[usize],
    palette: &[u32],
) -> u32 {
    let base = height - source_rows;
    let walk = (row_idx as f32 / SKY_JITTER_WALK_RATE).min((source_rows - 1) as f32);
    // Reversed: at row_idx=0 we start at source_rows-1 (bottom row) and walk
    // upward.
    let raw = ((source_rows - 1) as f32 - walk - drift).clamp(0.0, (source_rows - 1) as f32);
    let a = base + raw as usize;
    let b = (a + 1).min(base + source_rows - 1);
    lerp_color(
        resolve(data, col, a, height, colourmap, palette),
        resolve(data, col, b, height, colourmap, palette),
        raw.fract(),
    )
}

/// Average `count` rows starting at `row_start` across all columns.
fn avg_rows_color(
    data: &[usize],
    width: usize,
    row_start: usize,
    count: usize,
    height: usize,
    colourmap: &[usize],
    palette: &[u32],
) -> u32 {
    let mut sum = [0u32; 3];
    let mut n = 0u32;
    for col in 0..width {
        for row in row_start..row_start + count {
            let p = resolve(data, col, row, height, colourmap, palette);
            if p != 0 {
                sum[0] += (p >> 16) as u8 as u32;
                sum[1] += (p >> 8) as u8 as u32;
                sum[2] += p as u8 as u32;
                n += 1;
            }
        }
    }
    if n > 0 {
        ((sum[0] / n) << 16) | ((sum[1] / n) << 8) | (sum[2] / n)
    } else {
        0
    }
}

fn build_drift(width: usize, rows: usize, max_drift: f32, smoothness: f32, seed: u32) -> Vec<f32> {
    let mut drift = vec![0.0f32; width * rows];
    for col in 0..width {
        let mut current = 0.0f32;
        for r in 0..rows {
            let n = hrand(col as u32, r as u32, seed);
            let step = if n < smoothness {
                -0.3
            } else if n > (1.0 - smoothness) {
                0.3
            } else {
                0.0
            };
            current = (current + step).clamp(-max_drift, max_drift);
            drift[col * rows + r] = current;
        }
    }
    drift
}

pub(crate) fn build_sky_combined(
    data: &[usize],
    width: usize,
    height: usize,
    colourmap: &[usize],
    palette: &[u32],
) -> Vec<u32> {
    let up_rows = SKY_EXTEND_ROWS;
    let dn_rows = SKY_DOWN_ROWS;
    let source_rows = SKY_SOURCE_ROWS.min(height / 2).max(2);
    let zenith_rows = SKY_ZENITH_ROWS.min(source_rows).max(1);

    let fade_start = (up_rows as f32 * SKY_FADE_START_FRAC).round() as usize;
    let fade_end =
        ((fade_start as f32 + up_rows as f32 * SKY_FADE_SPAN_FRAC).round() as usize).min(up_rows);

    let zenith = avg_rows_color(data, width, 0, zenith_rows, height, colourmap, palette);
    let nadir = avg_rows_color(
        data,
        width,
        height - zenith_rows,
        zenith_rows,
        height,
        colourmap,
        palette,
    );

    let drift_up = build_drift(
        width,
        up_rows,
        SKY_MAX_DRIFT,
        SKY_DRIFT_SMOOTHNESS,
        0xD71F_7000,
    );
    let drift_dn = build_drift(
        width,
        dn_rows,
        SKY_MAX_DRIFT,
        SKY_DRIFT_SMOOTHNESS,
        0xA3C8_1F00,
    );

    // Upward extension
    let mut ext_up = vec![0u32; width * up_rows];
    for col in 0..width {
        for r in 0..up_rows {
            let d = drift_up[col * up_rows + r];
            let jitter = jitter_sample_up(data, col, r, d, source_rows, height, colourmap, palette);
            ext_up[col * up_rows + r] = if r < fade_start {
                jitter
            } else if r < fade_end {
                let t = smoothstep((r - fade_start) as f32 / (fade_end - fade_start) as f32);
                lerp_color(jitter, zenith, t)
            } else {
                zenith
            };
        }
    }

    // Downward extension (mirrors upward, fade toward nadir)
    let mut ext_dn = vec![0u32; width * dn_rows];
    for col in 0..width {
        for r in 0..dn_rows {
            let d = drift_dn[col * dn_rows + r];
            let jitter =
                jitter_sample_down(data, col, r, d, source_rows, height, colourmap, palette);
            ext_dn[col * dn_rows + r] = if r < fade_start {
                jitter
            } else if r < fade_end {
                let t = smoothstep((r - fade_start) as f32 / (fade_end - fade_start) as f32);
                lerp_color(jitter, nadir, t)
            } else {
                nadir
            };
        }
    }

    // Assemble: texture | up ext | down ext
    let total = height + up_rows + dn_rows;
    let mut combined = vec![0u32; width * total];
    for col in 0..width {
        let base = col * total;
        for row in 0..height {
            combined[base + row] = resolve(data, col, row, height, colourmap, palette);
        }
        for r in 0..up_rows {
            combined[base + height + r] = ext_up[col * up_rows + r];
        }
        for r in 0..dn_rows {
            combined[base + height + up_rows + r] = ext_dn[col * dn_rows + r];
        }
    }

    // Jitter bleeds into texture rows near each join
    let jitter_in_tex = SKY_JITTER_START_ROW.min(height / 2);
    for col in 0..width {
        let base = col * total;
        for i in 0..jitter_in_tex {
            let blend_t = 1.0 - (i as f32 + 1.0) / (jitter_in_tex as f32 + 1.0);
            // Top join bleed
            let d_up = drift_up[col * up_rows + i.min(up_rows - 1)];
            let jup = jitter_sample_up(data, col, i, d_up, source_rows, height, colourmap, palette);
            let idx = base + (i + 1);
            combined[idx] = lerp_color(combined[idx], jup, blend_t);
            // Bottom join bleed
            let d_dn = drift_dn[col * dn_rows + i.min(dn_rows - 1)];
            let jdn =
                jitter_sample_down(data, col, i, d_dn, source_rows, height, colourmap, palette);
            let idx = base + (height - 1 - i);
            combined[idx] = lerp_color(combined[idx], jdn, blend_t);
        }
    }

    combined
}
