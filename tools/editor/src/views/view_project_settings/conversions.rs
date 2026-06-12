//! `NodesFormat` Rust↔Slint conversions.

use editor_core::NodesFormat;

use crate::generated;

impl From<NodesFormat> for generated::NodesFormat {
    fn from(f: NodesFormat) -> Self {
        match f {
            NodesFormat::Room4Doom => Self::Room4doom,
            NodesFormat::Classic => Self::Classic,
            NodesFormat::Both => Self::Both,
        }
    }
}

impl From<generated::NodesFormat> for NodesFormat {
    fn from(f: generated::NodesFormat) -> Self {
        match f {
            generated::NodesFormat::Room4doom => Self::Room4Doom,
            generated::NodesFormat::Classic => Self::Classic,
            generated::NodesFormat::Both => Self::Both,
        }
    }
}
