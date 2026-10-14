//! Map-local drafts and derived UI state. The GPUI map entity owns this bundle.
use super::inspectors::{
    draw::DrawSettingsData, line::LineInspectorData, sector::SectorInspectorData,
    thing::ThingInspectorData,
};
use super::{canvas::CanvasData, status::StatusData, toolbar::ToolbarData};
use crate::boundary::Tool;
use crate::level_editor::ThingTemplate;
use editor_core::{LineKey, SectorKey};
use std::time::Instant;

pub(crate) type SyncKey = (u64, Option<SectorKey>, Tool, ThingTemplate);
pub(crate) struct MapPresentation {
    pub(crate) canvas: CanvasData,
    pub(crate) line: LineInspectorData,
    pub(crate) sector: SectorInspectorData,
    pub(crate) thing: ThingInspectorData,
    pub(crate) draw_settings: DrawSettingsData,
    pub(crate) toolbar: ToolbarData,
    pub(crate) status: StatusData,
    pub(crate) panels_key: Option<SyncKey>,
    pub(crate) panels_epoch: u64,
    pub(crate) hovered_line: Option<LineKey>,
    pub(crate) hovered_sector: Option<SectorKey>,
    pub(crate) hover_deadline: Option<Instant>,
    pub(crate) camera_deadline: Option<Instant>,
    pub(crate) light_deadline: Option<Instant>,
    pub(crate) bsp_deadline: Option<Instant>,
    pub(crate) bsp_interval: std::time::Duration,
    pub(crate) pixel_ratio: f32,
    pub(crate) document: u64,
    pub(crate) resources_epoch: u64,
}
impl Default for MapPresentation {
    fn default() -> Self {
        Self {
            canvas: Default::default(),
            line: Default::default(),
            sector: Default::default(),
            thing: Default::default(),
            draw_settings: Default::default(),
            toolbar: Default::default(),
            status: Default::default(),
            panels_key: None,
            panels_epoch: 0,
            hovered_line: None,
            hovered_sector: None,
            hover_deadline: None,
            camera_deadline: None,
            light_deadline: None,
            bsp_deadline: None,
            bsp_interval: std::time::Duration::from_millis(16),
            pixel_ratio: 1.,
            document: u64::MAX,
            resources_epoch: u64::MAX,
        }
    }
}
