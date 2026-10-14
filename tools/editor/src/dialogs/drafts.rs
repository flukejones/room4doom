//! Plain modal drafts. Their lifetime belongs to the native modal entity.

use super::{
    animations::AnimationsDraft,
    asset_picker::{AssetPickerDraft, actions::TexBrowseTarget},
    audit::{AuditDraft, actions::AuditTarget},
    build_bsp::BuildBspDraft,
    choose_map::MapsDraft,
    new_map::NewMapDraft,
    preferences::PreferencesDraft,
    project_settings::ProjectSettingsDraft,
    remap::RemapDraft,
    sector::{SectorDraft, actions::SectorEditDraft},
    wall::{WallDraft, actions::WallEditDraft},
};
use crate::components::types::{PopupGeom, PopupId};
use crate::level_editor::remap::RemapPair;

#[derive(Default)]
pub(crate) struct DialogDrafts {
    pub(super) animations: AnimationsDraft,
    pub(crate) document_generation: u64,
    pub(crate) wall: WallDraft,
    pub(crate) sector: SectorDraft,
    pub(crate) prefs: PreferencesDraft,
    pub(crate) asset_picker: AssetPickerDraft,
    pub(crate) project_settings: ProjectSettingsDraft,
    pub(crate) build_bsp: BuildBspDraft,
    pub(crate) maps: MapsDraft,
    pub(crate) new_map: NewMapDraft,
    pub(crate) audit: AuditDraft,
    pub(crate) remap: RemapDraft,
    pub(crate) wall_edit: Option<WallEditDraft>,
    pub(crate) sector_edit: Option<SectorEditDraft>,
    pub(crate) browse_target: Option<TexBrowseTarget>,
    pub(crate) browse_current: Option<String>,
    pub(crate) audit_targets: Vec<AuditTarget>,
    pub(crate) remap_pairs: Vec<RemapPair>,
    pub(crate) geometry: Vec<(PopupId, PopupGeom)>,
    pub(crate) pixel_ratio: f32,
}
