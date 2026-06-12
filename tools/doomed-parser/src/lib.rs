//! DoomEd ASCII format parsers for the map editor: `.dwd` maps (read-only
//! import) and `.dsp`/`.dpr` project definitions (read/write). Both sit on a
//! shared whitespace-skipping text [`cursor`]. Map/name types come from
//! `geom-kernel`; this crate adds only the format layer.

pub mod cursor;
pub mod dsp;
pub mod dwd;

pub use cursor::{Cursor, CursorError};
pub use dsp::{
    AnimDef, DESC_MAX_LEN, DspError, PatchPlacement, SpecialDef, TextureDef, ThingDef,
    parse_animated_dsp, parse_specials_dsp, parse_textures_dsp, parse_things_dsp,
    write_animated_dsp, write_specials_dsp, write_textures_dsp, write_things_dsp,
};
pub use dwd::{DWD_HEADER, DWD_VERSION, DwdError, THING_SNAP_MASK, parse_dwd};
