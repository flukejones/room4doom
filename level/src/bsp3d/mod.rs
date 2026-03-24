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
    AABB, BSP3D, BSPLeaf3D, MovementType, Node3D, OcclusionSeg, SurfaceKind, SurfacePolygon, WallTexPin, WallType
};
pub use movers::is_sector_mover;
