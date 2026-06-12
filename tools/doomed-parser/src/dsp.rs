//! DoomEd project DSP files (`*.dsp` in the project directory).
//!
//! Grammars (printf templates; whitespace between tokens is flexible where
//! `fscanf` would accept it):
//!
//! ```text
//! things.dsp:
//!   numthings: {N}
//!   {name} = {angle} {value} {option} ({r} {g} {b}) {iconname}
//!
//! sectorspecials.dsp / linespecials.dsp:
//!   numspecials: {N}
//!   {value}:{desc}
//!
//! texture{N}.dsp:
//!   numtextures: {N}
//!   {name} {width}, {height}, {patchcount}
//!      ({originx}, {originy} : {patchname} ) {stepdir}, {colormap}
//!
//! animated.dsp:
//!   numanims: {N}
//!   {tex|flat} {start} {end} {speed}
//! ```
//!
//! Names are ≤ 8 bytes; thing names and special descriptions are ≤ 31 bytes
//! and contain no whitespace (DoomEd reads them with `%31s`). Colors write
//! with six decimals, matching C's `%f`.

use std::fmt;
use std::io;

use geom_kernel::Name8;
use serde::{Deserialize, Serialize};

use crate::cursor::{Cursor, CursorError};

/// Maximum bytes in a thing name or special description (`char[32]`).
pub const DESC_MAX_LEN: usize = 31;

/// One entry of `things.dsp`: a placeable thing type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThingDef {
    pub name: String,
    pub angle: i32,
    /// Doom thing type number.
    pub value: i32,
    pub option: i32,
    /// Editor display color, each channel 0.0..=1.0.
    pub color: [f32; 3],
    /// Patch name shown in palettes (never drawn on the map canvas).
    pub icon: Name8,
}

/// One entry of `sectorspecials.dsp` or `linespecials.dsp`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecialDef {
    pub value: i32,
    pub desc: String,
}

/// One entry of `animated.dsp`: a flat or texture animation sequence that
/// cycles every frame from `start` to `end` (inclusive, by lump order).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimDef {
    /// `true` animates wall textures, `false` animates flats.
    pub is_texture: bool,
    pub start: Name8,
    pub end: Name8,
    /// Boom ANIMATED speed: game tics between frame advances.
    pub speed: i32,
}

/// A patch placement within a composite texture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PatchPlacement {
    pub origin_x: i32,
    pub origin_y: i32,
    pub patch: Name8,
    pub step_dir: i32,
    pub colormap: i32,
}

/// A composite texture definition (`texture{N}.dsp` record).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureDef {
    pub name: Name8,
    pub width: i32,
    pub height: i32,
    pub patches: Vec<PatchPlacement>,
}

/// Failure while reading a `.dsp` file.
#[derive(Debug)]
pub enum DspError {
    Io(io::Error),
    Parse {
        line: usize,
        expected: &'static str,
    },
    /// A field cannot be written because it would not parse back: too long, or
    /// (for `%31s` fields) contains whitespace.
    Encode {
        field: &'static str,
        value: String,
    },
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "dsp io error: {e}"),
            Self::Parse {
                line,
                expected,
            } => write!(f, "line {line}: expected {expected}"),
            Self::Encode {
                field,
                value,
            } => write!(
                f,
                "{field} {value:?} cannot be written (max {DESC_MAX_LEN} bytes, no whitespace)"
            ),
        }
    }
}

impl std::error::Error for DspError {}

impl From<io::Error> for DspError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl CursorError for DspError {
    fn unexpected_eof(line: usize) -> Self {
        Self::Parse {
            line,
            expected: "more input",
        }
    }

    fn bad_token(line: usize, expected: &'static str, _found: String) -> Self {
        Self::Parse {
            line,
            expected,
        }
    }

    fn bad_name(line: usize, _name: String) -> Self {
        Self::Parse {
            line,
            expected: "8-byte name",
        }
    }
}

/// Parse `things.dsp` text.
pub fn parse_things_dsp(text: &str) -> Result<Vec<ThingDef>, DspError> {
    let mut c = Cursor::<DspError>::new(text);
    c.lit("numthings:")?;
    let count = c.int()?.max(0) as usize;
    let mut defs = Vec::with_capacity(count);
    for _ in 0..count {
        let name = c.desc(DESC_MAX_LEN)?;
        c.lit("=")?;
        let angle = c.int()?;
        let value = c.int()?;
        let option = c.int()?;
        c.lit("(")?;
        let color = [c.float()?, c.float()?, c.float()?];
        c.lit(")")?;
        let icon = c.name8()?;
        defs.push(ThingDef {
            name,
            angle,
            value,
            option,
            color,
            icon,
        });
    }
    c.end()?;
    Ok(defs)
}

/// Validate a `%31s` field (≤ [`DESC_MAX_LEN`] bytes, no whitespace) so a
/// written DSP file parses back unchanged.
fn check_token(field: &'static str, value: &str) -> Result<(), DspError> {
    if value.len() > DESC_MAX_LEN || value.chars().any(char::is_whitespace) {
        return Err(DspError::Encode {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

/// Serialize `things.dsp` text. Errors if a thing name would not parse back.
pub fn write_things_dsp(defs: &[ThingDef]) -> Result<String, DspError> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let count = defs.len();
    writeln!(out, "numthings: {count}").expect("String formatting is infallible");
    for d in defs {
        check_token("thing name", &d.name)?;
        let (name, angle, value, option) = (&d.name, d.angle, d.value, d.option);
        let [r, g, b] = d.color;
        let icon = d.icon.to_dwd_field();
        writeln!(
            out,
            "{name} = {angle} {value} {option} ({r:.6} {g:.6} {b:.6}) {icon}"
        )
        .expect("String formatting is infallible");
    }
    Ok(out)
}

/// Parse `sectorspecials.dsp` / `linespecials.dsp` text (shared grammar).
pub fn parse_specials_dsp(text: &str) -> Result<Vec<SpecialDef>, DspError> {
    let mut c = Cursor::<DspError>::new(text);
    c.lit("numspecials:")?;
    let count = c.int()?.max(0) as usize;
    let mut defs = Vec::with_capacity(count);
    for _ in 0..count {
        let value = c.int()?;
        c.lit(":")?;
        let desc = c.desc(DESC_MAX_LEN)?;
        defs.push(SpecialDef {
            value,
            desc,
        });
    }
    c.end()?;
    Ok(defs)
}

/// Serialize specials text (both sector and line lists). Errors if a
/// description would not parse back.
pub fn write_specials_dsp(defs: &[SpecialDef]) -> Result<String, DspError> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let count = defs.len();
    writeln!(out, "numspecials: {count}").expect("String formatting is infallible");
    for d in defs {
        check_token("special desc", &d.desc)?;
        let (value, desc) = (d.value, &d.desc);
        writeln!(out, "{value}:{desc}").expect("String formatting is infallible");
    }
    Ok(out)
}

/// The `tex`/`flat` keyword tokens in `animated.dsp`.
const ANIM_KIND_TEXTURE: &str = "tex";
const ANIM_KIND_FLAT: &str = "flat";

/// Parse `animated.dsp` text.
pub fn parse_animated_dsp(text: &str) -> Result<Vec<AnimDef>, DspError> {
    let mut c = Cursor::<DspError>::new(text);
    c.lit("numanims:")?;
    let count = c.int()?.max(0) as usize;
    let mut defs = Vec::with_capacity(count);
    for _ in 0..count {
        let kind_line = {
            c.skip_ws();
            c.line
        };
        let is_texture = match c.token()? {
            ANIM_KIND_TEXTURE => true,
            ANIM_KIND_FLAT => false,
            _ => {
                return Err(DspError::Parse {
                    line: kind_line,
                    expected: "`tex` or `flat`",
                });
            }
        };
        let start = c.name8()?;
        let end = c.name8()?;
        let speed = c.int()?;
        defs.push(AnimDef {
            is_texture,
            start,
            end,
            speed,
        });
    }
    c.end()?;
    Ok(defs)
}

/// Serialize `animated.dsp` text.
pub fn write_animated_dsp(defs: &[AnimDef]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let count = defs.len();
    writeln!(out, "numanims: {count}").expect("String formatting is infallible");
    for d in defs {
        let kind = if d.is_texture {
            ANIM_KIND_TEXTURE
        } else {
            ANIM_KIND_FLAT
        };
        let (start, end, speed) = (d.start.to_dwd_field(), d.end.to_dwd_field(), d.speed);
        writeln!(out, "{kind} {start} {end} {speed}").expect("String formatting is infallible");
    }
    out
}

/// Parse a texture DSP file (`texture{N}.dsp`) or an inline `.dpr` texture
/// section beginning at `numtextures:`.
pub fn parse_textures_dsp(text: &str) -> Result<Vec<TextureDef>, DspError> {
    let mut c = Cursor::<DspError>::new(text);
    let defs = parse_textures_section(&mut c)?;
    c.end()?;
    Ok(defs)
}

fn parse_textures_section(c: &mut Cursor<'_, DspError>) -> Result<Vec<TextureDef>, DspError> {
    c.lit("numtextures:")?;
    let count = c.int()?.max(0) as usize;
    let mut defs = Vec::with_capacity(count);
    for _ in 0..count {
        let name = c.name8()?;
        let width = c.int()?;
        c.lit(",")?;
        let height = c.int()?;
        c.lit(",")?;
        let patch_count = c.int()?.max(0) as usize;
        let mut patches = Vec::with_capacity(patch_count);
        for _ in 0..patch_count {
            c.lit("(")?;
            let origin_x = c.int()?;
            c.lit(",")?;
            let origin_y = c.int()?;
            c.lit(":")?;
            let patch = c.name8()?;
            c.lit(")")?;
            let step_dir = c.int()?;
            c.lit(",")?;
            let colormap = c.int()?;
            patches.push(PatchPlacement {
                origin_x,
                origin_y,
                patch,
                step_dir,
                colormap,
            });
        }
        defs.push(TextureDef {
            name,
            width,
            height,
            patches,
        });
    }
    Ok(defs)
}

/// Serialize a texture DSP file.
pub fn write_textures_dsp(defs: &[TextureDef]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let count = defs.len();
    writeln!(out, "numtextures: {count}").expect("String formatting is infallible");
    for t in defs {
        let (name, width, height, patches) = (t.name.as_str(), t.width, t.height, t.patches.len());
        writeln!(out, "{name} {width}, {height}, {patches}")
            .expect("String formatting is infallible");
        for p in &t.patches {
            let (x, y, patch, step, cmap) = (
                p.origin_x,
                p.origin_y,
                p.patch.to_dwd_field(),
                p.step_dir,
                p.colormap,
            );
            writeln!(out, "   ({x}, {y} : {patch} ) {step}, {cmap}")
                .expect("String formatting is infallible");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;

    #[test]
    fn dsp_types_serde_round_trip() {
        let thing = ThingDef {
            name: "Imp".to_owned(),
            angle: 90,
            value: 3001,
            option: 7,
            color: [0.75, 0.375, 0.125],
            icon: Name8::from_dwd_field("TROOA1").unwrap_or(Name8::EMPTY),
        };
        let special = SpecialDef {
            value: 9,
            desc: "secret".to_owned(),
        };
        let tex = TextureDef {
            name: Name8::from_dwd_field("STARTAN1").unwrap_or(Name8::EMPTY),
            width: 128,
            height: 128,
            patches: vec![PatchPlacement {
                origin_x: 0,
                origin_y: 0,
                patch: Name8::from_dwd_field("WALL00_3").unwrap_or(Name8::EMPTY),
                step_dir: 1,
                colormap: 0,
            }],
        };
        let anim = AnimDef {
            is_texture: true,
            start: Name8::from_dwd_field("BLODGR1").unwrap_or(Name8::EMPTY),
            end: Name8::from_dwd_field("BLODGR4").unwrap_or(Name8::EMPTY),
            speed: 8,
        };
        assert_eq!(thing, ron_round_trip(&thing));
        assert_eq!(special, ron_round_trip(&special));
        assert_eq!(tex, ron_round_trip(&tex));
        assert_eq!(anim, ron_round_trip(&anim));
    }

    fn ron_round_trip<T>(value: &T) -> T
    where
        T: Serialize + DeserializeOwned,
    {
        let text = ron::to_string(value).expect("serialises");
        ron::from_str(&text).expect("deserialises")
    }

    #[test]
    fn things_round_trip() {
        let text = "numthings: 2\nPlayer1 = 90 1 7 (0.200000 0.800000 0.200000) PLAYA1\nShotgun = 0 2001 7 (0.500000 0.500000 0.000000) SHOTA0\n";
        let defs = parse_things_dsp(text).expect("things parse");
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "Player1");
        assert_eq!(defs[0].value, 1);
        assert_eq!(defs[0].color, [0.2, 0.8, 0.2]);
        assert_eq!(defs[1].icon.as_str(), "SHOTA0");
        assert_eq!(write_things_dsp(&defs).expect("things write"), text);
    }

    #[test]
    fn write_rejects_overlong_or_whitespace_fields() {
        let long = ThingDef {
            name: "x".repeat(DESC_MAX_LEN + 1),
            angle: 0,
            value: 1,
            option: 7,
            color: [0.0, 0.0, 0.0],
            icon: Name8::EMPTY,
        };
        assert!(matches!(
            write_things_dsp(std::slice::from_ref(&long)),
            Err(DspError::Encode { .. })
        ));
        let spaced = SpecialDef {
            value: 1,
            desc: "has space".to_owned(),
        };
        assert!(matches!(
            write_specials_dsp(std::slice::from_ref(&spaced)),
            Err(DspError::Encode { .. })
        ));
    }

    #[test]
    fn specials_round_trip() {
        let text = "numspecials: 2\n1:Door_OpenWaitClose\n48:Scroll_Left\n";
        let defs = parse_specials_dsp(text).expect("specials parse");
        assert_eq!(
            defs[0],
            SpecialDef {
                value: 1,
                desc: "Door_OpenWaitClose".to_owned()
            }
        );
        assert_eq!(write_specials_dsp(&defs).expect("specials write"), text);
    }

    #[test]
    fn textures_round_trip() {
        let text = "numtextures: 1\nSTARTAN3 128, 128, 2\n   (0, 0 : SW17_4 ) 1, 0\n   (64, 0 : SW17_5 ) 1, 0\n";
        let defs = parse_textures_dsp(text).expect("textures parse");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name.as_str(), "STARTAN3");
        assert_eq!(defs[0].patches.len(), 2);
        assert_eq!(defs[0].patches[1].origin_x, 64);
        assert_eq!(write_textures_dsp(&defs), text);
    }

    #[test]
    fn animated_round_trips() {
        let text = "numanims: 2\nflat NUKAGE1 NUKAGE3 8\ntex BLODGR1 BLODGR4 8\n";
        let defs = parse_animated_dsp(text).expect("animated parse");
        assert_eq!(defs.len(), 2);
        assert!(!defs[0].is_texture);
        assert_eq!(defs[0].start.as_str(), "NUKAGE1");
        assert_eq!(defs[0].end.as_str(), "NUKAGE3");
        assert_eq!(defs[0].speed, 8);
        assert!(defs[1].is_texture);
        assert_eq!(write_animated_dsp(&defs), text);
    }

    #[test]
    fn animated_bad_kind_rejected_with_line() {
        let text = "numanims: 1\nwall FOO BAR 8\n";
        let err = parse_animated_dsp(text).expect_err("bad kind");
        assert!(
            matches!(
                err,
                DspError::Parse {
                    line: 2,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn over_long_desc_rejected_with_line() {
        let text = "numspecials: 1\n1:This_description_is_way_too_long_for_doomed\n";
        let err = parse_specials_dsp(text).expect_err("32+ byte desc");
        assert!(
            matches!(
                err,
                DspError::Parse {
                    line: 2,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn truncated_patch_record_rejected() {
        let text = "numtextures: 1\nSTARTAN3 128, 128, 1\n   (0, 0 : SW17_4\n";
        let err = parse_textures_dsp(text).expect_err("missing close paren");
        assert!(matches!(err, DspError::Parse { .. }), "{err}");
    }
}
