//! Editor asset model: palette, colormaps, textures, patches, animations; no `pic-data` dep, palette/colormaps from raw `PLAYPAL`/`COLORMAP` lumps.

pub mod palette;
pub mod patch;
pub mod texture;
pub mod texture_compose;

pub use patch::{FLAT_SIDE, FlatPic, WallPic};
pub use texture_compose::{
    MISSING_PATCH_INDEX, MISSING_PATCH_RGBA, TRANSPARENT_INDEX, compose_texture_indices,
    decode_patch, encode_patch, patch_dims, resolve_patch_lump,
};

use std::collections::{HashMap, HashSet};
use std::mem;
use std::path::PathBuf;

use editor_core::{
    AnimDef, EditorMap, ImportedPatch, Name8, Project, TextureDef, TextureGroup, TextureMode,
    import_wad_texture_groups,
};
use wad::WadData;
use wad::types::WadPalette;

use self::palette::{COLORMAP_LEVELS, load_colormaps, load_palette};

/// Monotonic edit counters for the editable asset kinds (the palette is load-time fixed); caches use these to detect staleness.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetGen {
    pub patches: u64,
    pub textures: u64,
    pub animations: u64,
}

/// Whether assets live in an open project or a projectless IWAD draft.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetSource {
    Project,
    Draft,
}

/// IWAD flat: name + decoded 64×64 indices.
pub(crate) struct IwadFlat {
    pub name: Name8,
    pub flat: FlatPic,
}

/// Kind of resource named by a [`MissingResource`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ResourceKind {
    /// Wall texture or flat.
    Texture,
    /// Patch lump a texture composes from.
    Patch,
}

impl ResourceKind {
    /// Lowercase label for the resources panel.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::Patch => "patch",
        }
    }
}

/// Map-referenced name the loaded WAD set does not provide.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct MissingResource {
    pub name: Name8,
    pub kind: ResourceKind,
}

/// Editable asset working set: palette, colormaps, textures, patches, animations, flats.
pub(crate) struct EditorAssets {
    /// PLAYPAL palette 0, gamma-free.
    palette: WadPalette,
    /// COLORMAP light levels (0=bright..31=dark), raw index bytes.
    colormaps: Vec<[u8; 256]>,
    /// One group per source WAD + `TEXTURE<n>` lump, IWAD-first load order.
    texture_groups: Vec<TextureGroup>,
    active_group: usize,
    /// Custom: merge all WADs by name. Vanilla: scope to `active_map_wad`.
    texture_mode: TextureMode,
    /// Map's target WAD basename; scopes the index in Vanilla mode.
    active_map_wad: String,
    /// Name → `(group, def)` winner for `active_map_wad`. Rebuilt on map-wad/edit changes.
    texture_index: HashMap<Name8, (usize, usize)>,
    /// Patches flattened to palette indices per resolved texture (lazy, like `init_wall_pics`); evicted on edit, cleared on map-wad/group change.
    composed: HashMap<Name8, WallPic>,
    animations: Vec<AnimDef>,
    imported_patches: Vec<ImportedPatch>,
    iwad_flats: Vec<IwadFlat>,
    /// Flat name → index into `iwad_flats`; built once (flats are immutable).
    iwad_flat_index: HashMap<Name8, usize>,
    source: AssetSource,
    generation: AssetGen,
}

impl EditorAssets {
    /// Load assets from `wad_paths` (IWAD first); each path opens separately so `TEXTURE<n>` lumps get provenance-tagged groups, the merged `wad` view supplies palette/colormaps/flats/animations.
    pub fn load(wad_paths: &[PathBuf], wad: &WadData, project: Option<&Project>) -> Self {
        let palette = load_palette(wad).unwrap_or(WadPalette([wad::types::BLACK; 256]));
        let colormaps = load_colormaps(wad);

        let mut texture_groups = Vec::new();
        for path in wad_paths {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_owned();
            match WadData::try_new(path) {
                Ok(w) => texture_groups.extend(import_wad_texture_groups(&name, &w)),
                Err(e) => log::warn!("skipping unreadable WAD {}: {e}", path.display()),
            }
        }

        let iwad_flats = load_iwad_flats(wad);
        let iwad_flat_index = iwad_flats
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name, i))
            .collect();
        let animations_iwad = load_iwad_animations(wad);

        let (animations, imported_patches, source) = match project {
            Some(p) => {
                overlay_project_textures(&mut texture_groups, &p.textures);
                (
                    p.animations.clone(),
                    p.imported_patches.clone(),
                    AssetSource::Project,
                )
            }
            None => (animations_iwad, Vec::new(), AssetSource::Draft),
        };
        // Always at least one group so `active_group` indexing is safe.
        if texture_groups.is_empty() {
            texture_groups.push(TextureGroup {
                wad_name: String::new(),
                lump: Name8::new("TEXTURE1").expect("valid"),
                defs: Vec::new(),
                edited: true,
            });
        }

        let texture_mode = project.map_or_else(TextureMode::default, |p| p.settings.texture_mode);
        let mut assets = Self {
            palette,
            colormaps,
            texture_groups,
            active_group: 0,
            texture_mode,
            active_map_wad: String::new(),
            texture_index: HashMap::new(),
            composed: HashMap::new(),
            animations,
            imported_patches,
            iwad_flats,
            iwad_flat_index,
            source,
            generation: AssetGen::default(),
        };
        assets.rebuild_texture_index();
        assets
    }

    /// Flush edited texture groups + animations into `project`; sets source to `Project`.
    pub fn write_into(&mut self, project: &mut Project) {
        project.textures = self
            .texture_groups
            .iter()
            .filter(|g| g.edited)
            .cloned()
            .collect();
        project.animations = self.animations.clone();
        project.imported_patches = self.imported_patches.clone();
        self.source = AssetSource::Project;
    }

    pub fn palette(&self) -> &WadPalette {
        &self.palette
    }

    /// COLORMAP table at `level` (0=bright..31=dark).
    pub fn colormap(&self, level: usize) -> Option<&[u8; 256]> {
        (level < COLORMAP_LEVELS)
            .then(|| self.colormaps.get(level))
            .flatten()
    }

    /// Texture defs of the active group.
    pub fn textures(&self) -> &[TextureDef] {
        &self.texture_groups[self.active_group].defs
    }

    /// Index of the active group.
    pub fn active_group(&self) -> usize {
        self.active_group
    }

    /// "wad · LUMP" label for group `i`.
    pub fn group_label(&self, i: usize) -> Option<String> {
        self.texture_groups.get(i).map(|g| {
            if g.wad_name.is_empty() {
                g.lump.as_str().to_owned()
            } else {
                format!("{} · {}", g.wad_name, g.lump.as_str())
            }
        })
    }

    /// Switch active group. No-op if out-of-range or unchanged.
    pub fn set_active_group(&mut self, i: usize) {
        if i >= self.texture_groups.len() || i == self.active_group {
            return;
        }
        self.active_group = i;
        self.generation.textures = self.generation.textures.wrapping_add(1);
        self.rebuild_texture_index();
    }

    pub fn imported_patches(&self) -> &[ImportedPatch] {
        &self.imported_patches
    }

    pub fn animations(&self) -> &[AnimDef] {
        &self.animations
    }

    /// Retarget the texture index to `map_wad` (Vanilla mode only). No-op if unchanged.
    pub fn set_map_wad(&mut self, map_wad: &str) {
        if self.texture_mode != TextureMode::Vanilla
            || self.active_map_wad.eq_ignore_ascii_case(map_wad)
        {
            return;
        }
        self.active_map_wad = map_wad.to_owned();
        self.rebuild_texture_index();
    }

    /// True if the current map WAD defines `name`.
    pub fn map_texture_exists(&self, name: &Name8) -> bool {
        self.texture_index.contains_key(name)
    }

    /// Compose uncached names into wall pics (lazy; skips already-cached). Call before sampling `composed`.
    pub fn ensure_composed(&mut self, names: &[Name8], wad: &WadData) {
        for name in names {
            if self.composed.contains_key(name) {
                continue;
            }
            if let Some(&(g, t)) = self.texture_index.get(name)
                && let Some(def) = self.texture_groups.get(g).and_then(|grp| grp.defs.get(t))
            {
                let pic = compose_texture_indices(def, &self.imported_patches, wad);
                self.composed.insert(*name, pic);
            }
        }
    }

    /// Cached composed pic, or `None` if not yet composed or map WAD doesn't define it.
    pub fn composed(&self, name: &Name8) -> Option<&WallPic> {
        self.composed.get(name)
    }

    /// Rebuild name → `(group, def)` in reverse load order (later WAD/lump wins); Custom merges all groups, Vanilla scopes to `active_map_wad` groups; clears `composed`.
    fn rebuild_texture_index(&mut self) {
        self.texture_index.clear();
        self.composed.clear();
        let vanilla = self.texture_mode == TextureMode::Vanilla;
        for (gi, group) in self.texture_groups.iter().enumerate().rev() {
            if vanilla && !group.wad_name.eq_ignore_ascii_case(&self.active_map_wad) {
                continue;
            }
            for (ti, def) in group.defs.iter().enumerate() {
                self.texture_index.entry(def.name).or_insert((gi, ti));
            }
        }
    }

    pub fn iwad_flats(&self) -> &[IwadFlat] {
        &self.iwad_flats
    }

    /// Index of the flat named `name`.
    pub fn iwad_flat_num(&self, name: &Name8) -> Option<usize> {
        self.iwad_flat_index.get(name).copied()
    }

    /// Resolved [`TextureDef`] for `name`, or `None` if not defined by the map WAD.
    pub fn texture_def(&self, name: &Name8) -> Option<&TextureDef> {
        let &(g, t) = self.texture_index.get(name)?;
        let grp = self.texture_groups.get(g)?;
        grp.defs.get(t)
    }

    /// Map-referenced names missing from the WAD set, derived live: unresolved wall texture/flat → `Texture`, resolved texture with absent patch lump → `Patch` (patches of unresolved textures are unknowable); deduped, name-sorted.
    pub fn missing_resources(&self, map: &EditorMap, wad: &WadData) -> Vec<MissingResource> {
        let mut seen: HashSet<Name8> = HashSet::new();
        let mut missing = Vec::new();
        let mut report = |name: Name8, kind| {
            if seen.insert(name) {
                missing.push(MissingResource {
                    name,
                    kind,
                });
            }
        };
        for line in map.lines.values() {
            for side in line.sides() {
                for tex in [&side.top_tex, &side.middle_tex, &side.bottom_tex] {
                    if tex.is_empty() {
                        continue;
                    }
                    match self.texture_def(tex) {
                        None => report(*tex, ResourceKind::Texture),
                        Some(def) => {
                            for placement in &def.patches {
                                if resolve_patch_lump(
                                    placement.patch.as_str(),
                                    &self.imported_patches,
                                    wad,
                                )
                                .is_none()
                                {
                                    report(placement.patch, ResourceKind::Patch);
                                }
                            }
                        }
                    }
                }
            }
        }
        for sector in map.sectors.values() {
            for flat in [&sector.floor_flat, &sector.ceil_flat] {
                if !flat.is_empty() && self.iwad_flat_num(flat).is_none() {
                    report(*flat, ResourceKind::Texture);
                }
            }
        }
        missing.sort_unstable_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        missing
    }

    pub fn generation(&self) -> AssetGen {
        self.generation
    }

    pub(super) fn imported_patches_slice(&self) -> &[ImportedPatch] {
        &self.imported_patches
    }

    pub(super) fn imported_patches_push(&mut self, patch: ImportedPatch) {
        self.imported_patches.push(patch);
        self.generation.patches = self.generation.patches.wrapping_add(1);
    }

    /// Mutable defs of the active group; marks edited, bumps generation.
    pub(super) fn textures_vec_mut(&mut self) -> &mut Vec<TextureDef> {
        self.generation.textures = self.generation.textures.wrapping_add(1);
        let g = &mut self.texture_groups[self.active_group];
        g.edited = true;
        &mut g.defs
    }

    /// Swap active group's defs (undo/redo); returns the displaced defs.
    pub(super) fn replace_textures(&mut self, set: Vec<TextureDef>) -> Vec<TextureDef> {
        self.generation.textures = self.generation.textures.wrapping_add(1);
        let g = &mut self.texture_groups[self.active_group];
        g.edited = true;
        let prev = mem::replace(&mut g.defs, set);
        self.rebuild_texture_index();
        prev
    }

    /// Rebuild texture index after a `texture_mut`/`textures_vec_mut` batch (`&mut` callers can't self-rebuild).
    pub(super) fn refresh_texture_index(&mut self) {
        self.rebuild_texture_index();
    }

    pub(super) fn animations_vec_mut(&mut self) -> &mut Vec<AnimDef> {
        self.generation.animations = self.generation.animations.wrapping_add(1);
        &mut self.animations
    }

    /// Swap the animation set (undo/redo); returns the displaced set.
    pub(super) fn replace_animations(&mut self, set: Vec<AnimDef>) -> Vec<AnimDef> {
        self.generation.animations = self.generation.animations.wrapping_add(1);
        mem::replace(&mut self.animations, set)
    }
}

/// Overlay project groups over WAD-derived groups; same `(wad_name, lump)` replaces, else appends.
fn overlay_project_textures(groups: &mut Vec<TextureGroup>, project: &[TextureGroup]) {
    for pg in project {
        if let Some(slot) = groups.iter_mut().find(|g| {
            g.wad_name.eq_ignore_ascii_case(&pg.wad_name)
                && g.lump.as_str().eq_ignore_ascii_case(pg.lump.as_str())
        }) {
            *slot = pg.clone();
        } else {
            groups.push(pg.clone());
        }
    }
}

/// Decode all IWAD flats (raw 64×64 lump → index data); PWAD replacements win.
fn load_iwad_flats(wad: &WadData) -> Vec<IwadFlat> {
    let mut flats = Vec::new();
    let mut seen = HashSet::new();
    for wf in wad.flats_iter() {
        if !seen.insert(wf.name.clone()) {
            continue;
        }
        let Ok(name) = Name8::new(&wf.name) else {
            continue;
        };
        flats.push(IwadFlat {
            name,
            flat: FlatPic::from_lump(&wf.data),
        });
    }
    flats
}

/// Boom `ANIMATED` lump sequences; empty if absent. Vanilla hardcoded anims not included.
fn load_iwad_animations(wad: &WadData) -> Vec<AnimDef> {
    wad.get_lump("ANIMATED")
        .map(|l| {
            wad::boom::parse_animated(&l.data)
                .into_iter()
                .filter_map(|e| {
                    Some(AnimDef {
                        is_texture: e.is_texture,
                        start: Name8::new(&e.start_name).ok()?,
                        end: Name8::new(&e.end_name).ok()?,
                        speed: e.speed as i32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use editor_core::{DenseLineDef, DenseMap, DenseSideDef, LineFlags, Vertex};

    use super::*;

    fn group(wad: &str, lump: &str, names: &[(&str, &[&str])], edited: bool) -> TextureGroup {
        let defs = names
            .iter()
            .map(|(n, patches)| TextureDef {
                name: Name8::new(n).expect("name"),
                width: 64,
                height: 64,
                patches: patches
                    .iter()
                    .map(|p| editor_core::PatchPlacement {
                        origin_x: 0,
                        origin_y: 0,
                        patch: Name8::new(p).expect("patch"),
                        step_dir: 1,
                        colormap: 0,
                    })
                    .collect(),
            })
            .collect();
        TextureGroup {
            wad_name: wad.to_owned(),
            lump: Name8::new(lump).expect("lump"),
            defs,
            edited,
        }
    }

    fn assets_with(groups: Vec<TextureGroup>) -> EditorAssets {
        EditorAssets {
            palette: WadPalette([wad::types::BLACK; 256]),
            colormaps: Vec::new(),
            texture_groups: groups,
            active_group: 0,
            texture_mode: TextureMode::Vanilla,
            active_map_wad: String::new(),
            texture_index: HashMap::new(),
            composed: HashMap::new(),
            animations: Vec::new(),
            imported_patches: Vec::new(),
            iwad_flats: Vec::new(),
            iwad_flat_index: HashMap::new(),
            source: AssetSource::Draft,
            generation: AssetGen::default(),
        }
    }

    fn def_for<'a>(a: &'a EditorAssets, name: &str) -> Option<&'a TextureDef> {
        let n = Name8::new(name).expect("name");
        let &(g, t) = a.texture_index.get(&n)?;
        let grp = a.texture_groups.get(g)?;
        grp.defs.get(t)
    }

    #[test]
    fn index_overrides_by_later_lump_then_later_wad() {
        let mut a = assets_with(vec![
            group(
                "iwad.wad",
                "TEXTURE1",
                &[("BRICK", &["P1"]), ("WOOD", &["W1"])],
                false,
            ),
            group("iwad.wad", "TEXTURE2", &[("BRICK", &["P2"])], false),
        ]);
        a.set_map_wad("iwad.wad");
        assert_eq!(
            def_for(&a, "BRICK").unwrap().patches[0].patch.as_str(),
            "P2",
            "TEXTURE2 overrides TEXTURE1 within a WAD"
        );
        assert_eq!(
            def_for(&a, "WOOD").unwrap().patches[0].patch.as_str(),
            "W1",
            "name only in TEXTURE1 still resolves"
        );

        a.texture_groups
            .push(group("mod.wad", "TEXTURE1", &[("BRICK", &["P3"])], false));
        a.refresh_texture_index();
        assert_eq!(
            def_for(&a, "BRICK").unwrap().patches[0].patch.as_str(),
            "P2",
            "other WAD's group does not leak into this map's index"
        );

        a.set_map_wad("mod.wad");
        assert_eq!(
            def_for(&a, "BRICK").unwrap().patches[0].patch.as_str(),
            "P3",
            "the map's own WAD wins"
        );
        assert!(
            def_for(&a, "WOOD").is_none(),
            "mod.wad does not define WOOD → unresolved (no cross-WAD fallback)"
        );
    }

    #[test]
    fn custom_mode_merges_across_wads() {
        let mut a = assets_with(vec![
            group(
                "iwad.wad",
                "TEXTURE1",
                &[("BRICK", &["P1"]), ("WOOD", &["W1"])],
                false,
            ),
            group("mod.wad", "TEXTURE1", &[("BRICK", &["P3"])], false),
        ]);
        a.texture_mode = TextureMode::Custom;
        a.set_map_wad("mod.wad");
        a.refresh_texture_index();
        assert_eq!(
            def_for(&a, "BRICK").unwrap().patches[0].patch.as_str(),
            "P3",
            "later WAD's BRICK wins"
        );
        assert_eq!(
            def_for(&a, "WOOD").unwrap().patches[0].patch.as_str(),
            "W1",
            "IWAD WOOD falls through (not in the PWAD)"
        );
    }

    #[test]
    fn missing_resources_reports_unresolved_texture_and_absent_patch() {
        let n = |s| Name8::new(s).expect("name");
        let mut a = assets_with(vec![group(
            "iwad.wad",
            "TEXTURE1",
            &[("BRICK", &["GONEPAT"]), ("WALL01", &["P1"])],
            false,
        )]);
        a.set_map_wad("iwad.wad");

        let side = |mid| DenseSideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: mid,
            sector: None,
        };
        let map = EditorMap::from_dense(DenseMap {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                },
                Vertex {
                    x: 64.0,
                    y: 0.0,
                },
            ],
            lines: vec![DenseLineDef {
                v1: 0,
                v2: 1,
                flags: LineFlags::empty(),
                special: 0,
                tag: 0,
                front: side(n("BRICK")),
                back: Some(side(n("GONE"))),
            }],
            sectors: Vec::new(),
            things: Vec::new(),
            required_wads: Vec::new(),
        })
        .expect("fixture refs valid");

        let wad = WadData::new(&test_utils::doom1_wad_path());
        let missing = a.missing_resources(&map, &wad);

        assert!(
            missing
                .iter()
                .any(|m| m.name == n("GONE") && m.kind == ResourceKind::Texture),
            "an unresolved texture is reported"
        );
        assert!(
            missing
                .iter()
                .any(|m| m.name == n("GONEPAT") && m.kind == ResourceKind::Patch),
            "a resolved texture's absent patch is reported"
        );
        assert!(
            !missing.iter().any(|m| m.name == n("BRICK")),
            "BRICK resolves, so it is not missing"
        );
    }
}
