//! Visual asset data for rendering: textures, sprites, palettes, colourmaps,
//! and voxel models. Loaded from WAD and KVX/PK3 files.

pub mod colour;
pub mod parallel;
pub mod pic;
pub mod sky;
pub mod voxel;

pub use colour::{ByteOrder, PALETTE_LEN, PalLit, PalLitCache, PixelFmt, WadPalette};
pub use parallel::parallel_map;
pub use pic::sprites::{SpriteDef, SpriteFrame};
pub use pic::{
    CrtGamma, FlatPic, INVERSECOLORMAP, PaletteFade, PicAnimation, PicData, SpritePic, Switches,
    WallPic, player_cshift, resolve_tint_state,
};
pub use voxel::VoxelManager;
pub use voxel::faces::{VoxelFace, generate_faces};
pub use voxel::slices::{VoxelColumn, VoxelSliceQuad, VoxelSlices, VoxelSpan};
