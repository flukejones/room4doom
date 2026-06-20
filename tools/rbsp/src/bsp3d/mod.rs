//! 3D geometry builder: walls, floor/ceiling N-gons, sky fillers, and the
//! mover vertex pass, emitted as a flat serializable [`Bsp3dLump`].
//!
//! Consumes WAD-level records (via the accessor traits) plus a [`crate::BspOutput`]
//! — the engine's runtime structure is parsed from the lump by the `level`
//! crate.

pub mod builder;
pub mod derive;
pub mod input;
pub mod lump;
pub mod movers;

/// Bump when the builder's output changes for identical input — the engine
/// keys its lump cache on this, so stale caches rebuild.
pub const BUILDER_REVISION: u32 = 4;

pub use builder::{Bsp3dBuilder, HEIGHT_EPSILON, QUANT_PRECISION};
pub use derive::LeafBounds;
pub use input::Bsp3dInput;
pub use lump::{Bsp3dLump, LeafRecord, NO_INDEX, PolyFlags, PolyRecord, TreeNode, tree_from_nodes};
