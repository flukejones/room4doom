//! 3D BSP construction and mover vertex management.
//!
//! Submodules:
//! - [`build`]: [`BSP3D`] struct, construction, and runtime surface updates.
//! - [`movers`]: Post-construction mover vertex pass and AABB expansion.
//! - [`node`]: Extension methods on the raw [`Node`] type.

pub mod build;
pub mod movers;
pub mod node;

pub use build::{
    AABB, BSP3D, BSPLeaf3D, LIGHT_LEVELS, MovementType, Node3D, SurfaceKind, SurfacePolygon,
    WallFace, WallTexPin, WallType, contrast_adjust, light_band,
};
pub use movers::is_sector_mover;
