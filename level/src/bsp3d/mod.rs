//! 3D BSP runtime: parse a [`Bsp3dLump`] (built by `rbsp::bsp3d`, read from a
//! v3 `RBSP` lump or built at load) into the runtime [`BSP3D`].
//!
//! Submodules:
//! - [`movers`]: parse-side sector mover classification (AABB expansion).
//! - `parse`: [`BSP3D::from_lump`] — materializes the runtime structure.
//! - [`runtime`]: runtime [`BSP3D`] — render SoA + mover/texture event API.

pub mod movers;
mod parse;
pub mod runtime;

pub use rbsp::bsp3d::{
    Bsp3dBuilder, Bsp3dInput, Bsp3dLump, LeafRecord, NO_INDEX, PolyFlags, PolyRecord,
};
pub use runtime::{
    AABB, BSP3D, BSPLeaf3D, IS_LEAF_MASK, LIGHT_LEVELS, MovementType, Node3D, Polygon3D, WallSlot,
    contrast_adjust, is_leaf, leaf_index, light_band, mark_leaf,
};
