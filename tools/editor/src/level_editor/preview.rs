//! Wall bands shared by hover previews and wall inspectors.

use editor_core::{EditorMap, LineDef, Sector, SideDef};

use crate::boundary::TexSlot;
use crate::gfx::WallBand;

/// Wall bands for one side, cropped to the textured range top to bottom; `None` if none textured.
pub(crate) fn side_bands(
    map: &EditorMap,
    line: &LineDef,
    side: &SideDef,
    is_front: bool,
) -> Option<Vec<WallBand>> {
    let s = map.sectors.get(side.sector?)?;
    let other = if is_front {
        line.back.as_ref()
    } else {
        Some(&line.front)
    };
    let other = other
        .and_then(|o| o.sector)
        .and_then(|s| map.sectors.get(s));

    let mut raw: Vec<WallBand> = side_bands_tagged(s, other, side, is_front)
        .into_iter()
        .map(|(_, band)| band)
        .collect();
    let first = raw.iter().position(|b| b.tex.is_some())?;
    let last = raw.iter().rposition(|b| b.tex.is_some())?;
    let kept: Vec<WallBand> = raw.drain(first..=last).collect();
    Some(kept)
}

/// Like `side_bands` but tagged with `TexSlot` and uncropped.
#[expect(
    clippy::cast_precision_loss,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
pub(crate) fn side_bands_tagged(
    s: &Sector,
    other: Option<&Sector>,
    side: &SideDef,
    is_front: bool,
) -> Vec<(TexSlot, WallBand)> {
    let (top, mid, bottom) = if is_front {
        (TexSlot::FrontTop, TexSlot::FrontMid, TexSlot::FrontBottom)
    } else {
        (TexSlot::BackTop, TexSlot::BackMid, TexSlot::BackBottom)
    };

    let mut bands = Vec::with_capacity(3);
    match other {
        None => {
            let h = (s.ceil_height - s.floor_height) as f32;
            if h > 0.0 {
                bands.push((
                    mid,
                    WallBand {
                        tex: (!side.middle_tex.is_empty()).then_some(side.middle_tex),
                        height: h,
                        masked: false,
                    },
                ));
            }
        }
        Some(o) => {
            let upper = (s.ceil_height - o.ceil_height) as f32;
            if upper > 0.0 {
                bands.push((
                    top,
                    WallBand {
                        tex: (!side.top_tex.is_empty()).then_some(side.top_tex),
                        height: upper,
                        masked: false,
                    },
                ));
            }
            let middle =
                (s.ceil_height.min(o.ceil_height) - s.floor_height.max(o.floor_height)) as f32;
            if middle > 0.0 {
                bands.push((
                    mid,
                    WallBand {
                        tex: (!side.middle_tex.is_empty()).then_some(side.middle_tex),
                        height: middle,
                        masked: true,
                    },
                ));
            }
            let lower = (o.floor_height - s.floor_height) as f32;
            if lower > 0.0 {
                bands.push((
                    bottom,
                    WallBand {
                        tex: (!side.bottom_tex.is_empty()).then_some(side.bottom_tex),
                        height: lower,
                        masked: false,
                    },
                ));
            }
        }
    }
    bands
}
