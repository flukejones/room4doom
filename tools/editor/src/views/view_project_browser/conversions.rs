//! `LumpKind`, Slint mirror, classifier, row builders.

use std::collections::HashSet;
use std::path::Path;

use editor_core::Name8;
use slint::{ModelRc, VecModel};
use wad::WadData;

use crate::assets::EditorAssets;
use crate::generated::{LumpKind as UiLumpKind, LumpRow, TreeRow};
use crate::gfx::GfxCache;
use crate::views::view_project_browser::{LeafRef, LoadedWad, NodeKey, TreeNode};

/// Lump content kind; `LumpKind` in Slint is its mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LumpKind {
    MapMarker,
    MapSubLump,
    Flat,
    Patch,
    Sprite,
    Music,
    Sound,
    TextureDefs,
    Palette,
    Colourmap,
    Text,
    #[default]
    Other,
}

impl LumpKind {
    /// True for the kinds rendered as an image in the content view.
    pub fn is_image(self) -> bool {
        matches!(self, Self::Flat | Self::Patch | Self::Sprite)
    }
}

impl From<LumpKind> for UiLumpKind {
    fn from(k: LumpKind) -> Self {
        match k {
            LumpKind::MapMarker => Self::MapMarker,
            LumpKind::MapSubLump => Self::MapSubLump,
            LumpKind::Flat => Self::Flat,
            LumpKind::Patch => Self::Patch,
            LumpKind::Sprite => Self::Sprite,
            LumpKind::Music => Self::Music,
            LumpKind::Sound => Self::Sound,
            LumpKind::TextureDefs => Self::TextureDefs,
            LumpKind::Palette => Self::Palette,
            LumpKind::Colourmap => Self::Colourmap,
            LumpKind::Text => Self::Text,
            LumpKind::Other => Self::Other,
        }
    }
}

const MUS_MAGIC: &[u8; 4] = b"MUS\x1a"; // MUS lump magic
const DMX_TAG: [u8; 2] = [0x03, 0x00]; // DMX digital sound, LE u16=3

/// Classify lump by namespace markers, name, and magic bytes.
pub(super) fn classify(wad: &WadData, idx: usize) -> LumpKind {
    let lumps = wad.lumps();
    let Some(lump) = lumps.get(idx) else {
        return LumpKind::Other;
    };
    let name = lump.name.as_str();
    match name {
        "PLAYPAL" => return LumpKind::Palette,
        "COLORMAP" => return LumpKind::Colourmap,
        "TEXTURE1" | "TEXTURE2" => return LumpKind::TextureDefs,
        _ => {}
    }
    if is_map_marker(name) {
        return LumpKind::MapMarker;
    }
    if MAP_SUB_LUMPS.contains(&name) {
        return LumpKind::MapSubLump;
    }
    match namespace_at(lumps, idx) {
        Some(Namespace::Flats) => return LumpKind::Flat,
        Some(Namespace::Patches) => return LumpKind::Patch,
        Some(Namespace::Sprites) => return LumpKind::Sprite,
        None => {}
    }
    let data = lump.data.as_slice();
    if name.starts_with("D_") && data.len() >= 4 && &data[..4] == MUS_MAGIC {
        return LumpKind::Music;
    }
    if name.starts_with("DS") && data.len() >= 2 && data[..2] == DMX_TAG {
        return LumpKind::Sound;
    }
    if name.starts_with("DP") {
        return LumpKind::Sound;
    }
    LumpKind::Other
}

const MAP_SUB_LUMPS: [&str; 11] = [
    "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SEGS", "SSECTORS", "NODES", "SECTORS", "REJECT",
    "BLOCKMAP", "BEHAVIOR",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Namespace {
    Flats,
    Patches,
    Sprites,
}

fn namespace_at(lumps: &[wad::Lump], idx: usize) -> Option<Namespace> {
    let mut current = None;
    for lump in &lumps[..idx] {
        match lump.name.as_str() {
            "F_START" | "FF_START" => current = Some(Namespace::Flats),
            "P_START" | "PP_START" | "P1_START" | "P2_START" | "P3_START" => {
                current = Some(Namespace::Patches);
            }
            "S_START" | "SS_START" => current = Some(Namespace::Sprites),
            "F_END" | "FF_END" | "P_END" | "PP_END" | "P1_END" | "P2_END" | "P3_END" | "S_END"
            | "SS_END" => current = None,
            _ => {}
        }
    }
    current
}

fn is_map_marker(name: &str) -> bool {
    let b = name.as_bytes();
    (b.len() == 4 && b[0] == b'E' && b[1].is_ascii_digit() && b[2] == b'M' && b[3].is_ascii_digit())
        || (b.len() == 5 && &b[..3] == b"MAP" && b[3].is_ascii_digit() && b[4].is_ascii_digit())
}

pub(super) fn build_tree(
    wads: &[LoadedWad],
    project_dir: Option<&Path>,
    expanded: &HashSet<NodeKey>,
) -> Vec<TreeNode> {
    let mut nodes = Vec::new();
    for (i, loaded) in wads.iter().enumerate() {
        let tag = if loaded.is_iwad { " (iwad)" } else { " (pwad)" };
        // WADs are not expandable; select fills lump column.
        nodes.push(TreeNode {
            key: Some(NodeKey::Wad(i)),
            label: format!("{}{tag}", loaded.name),
            depth: 0,
            expandable: false,
            expanded: false,
            container: true,
            leaf: Some(LeafRef::Wad(i)),
        });
    }
    if let Some(dir) = project_dir {
        push_dir(&mut nodes, dir, 0, expanded);
    }
    nodes
}

fn push_dir(nodes: &mut Vec<TreeNode>, dir: &Path, depth: i32, expanded: &HashSet<NodeKey>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = path.is_dir();
        if is_dir {
            let key = NodeKey::Dir(path.clone());
            let open = expanded.contains(&key);
            nodes.push(TreeNode {
                key: Some(key),
                label: name,
                depth,
                expandable: true,
                expanded: open,
                container: false,
                leaf: None,
            });
            if open {
                push_dir(nodes, &path, depth + 1, expanded);
            }
        } else {
            nodes.push(TreeNode {
                key: None,
                label: name,
                depth,
                expandable: false,
                expanded: false,
                container: false,
                leaf: Some(LeafRef::File(path)),
            });
        }
    }
}

pub(super) fn tree_rows(nodes: &[TreeNode]) -> ModelRc<TreeRow> {
    let rows: Vec<TreeRow> = nodes
        .iter()
        .map(|n| TreeRow {
            label: n.label.as_str().into(),
            depth: n.depth,
            expandable: n.expandable,
            expanded: n.expanded,
            container: n.container,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(super) fn lump_rows(wads: &[LoadedWad], wad_idx: usize, name_filter: &str) -> ModelRc<LumpRow> {
    let Some(loaded) = wads.get(wad_idx) else {
        return ModelRc::new(VecModel::from(Vec::<LumpRow>::new()));
    };
    let rows: Vec<LumpRow> = loaded
        .wad
        .lumps()
        .iter()
        .enumerate()
        .map(|(i, lump)| LumpRow {
            name: lump.name.as_str().into(),
            size: lump.data.len() as i32,
            kind: classify(&loaded.wad, i).into(),
            matches: lump_matches(&lump.name, name_filter),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(super) fn lump_matches(name: &str, filter: &str) -> bool {
    filter.is_empty() || name.to_lowercase().contains(filter)
}

/// Decode image lump to `slint::Image`; `None` if undecodable.
pub(super) fn image_for(
    gfx: &mut GfxCache,
    assets: &EditorAssets,
    wad: &WadData,
    kind: LumpKind,
    name: &str,
) -> Option<slint::Image> {
    match kind {
        LumpKind::Flat => {
            let num = assets.iwad_flat_num(&Name8::new(name).ok()?)?;
            Some(gfx.flat_image(assets, num))
        }
        LumpKind::Patch | LumpKind::Sprite => gfx.patch_image(assets, wad, name),
        _ => None,
    }
}

pub(super) fn placeholder_text(kind: LumpKind, name: &str, size: usize) -> String {
    format!("{name} — {} preview ({size} bytes) — TODO", kind.label())
}

impl LumpKind {
    /// Short label for the placeholder content view.
    fn label(self) -> &'static str {
        match self {
            Self::MapMarker => "map",
            Self::MapSubLump => "map lump",
            Self::Flat => "flat",
            Self::Patch => "patch",
            Self::Sprite => "sprite",
            Self::Music => "music",
            Self::Sound => "sound",
            Self::TextureDefs => "texture defs",
            Self::Palette => "palette",
            Self::Colourmap => "colourmap",
            Self::Text => "text",
            Self::Other => "raw",
        }
    }
}
