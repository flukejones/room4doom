//! Typed map commands share the same domain path in native listeners and headless tests.
use super::inspectors::{actions as inspectors, draw};
use super::{presentation::MapPresentation, preview};
use crate::app::project::save_prefs_now;
use crate::app::session::EditorSession;
use crate::boundary::{FlatSlot, SelectMode, TexSlot, Tool};
use crate::components::types::{SectorFill as UiSectorFill, SkillFilter, ToolKind};
use crate::dialogs::{DialogRequest, asset_picker::actions::TexBrowseTarget};
use crate::level_editor::{
    pick3d::PickKind,
    state::{Damage, SectorFill},
};
use crate::render::camera3d::Projection;
use editor_core::{ArenaKey as _, Axis};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum MapCommand {
    ToolClick(f32, f32, bool),
    ContextMenuOpening(f32, f32),
    ToolDragStart(f32, f32, bool),
    ToolDrag(f32, f32),
    ToolDragEnd(f32, f32),
    LineDragMove,
    LineDragExtrude,
    LineDragCancel,
    Fillet(i32, i32),
    Chamfer(i32),
    Hover(f32, f32),
    DoubleClicked(f32, f32),
    Pan(f32, f32),
    ZoomAt(f32, f32, f32),
    Orbit(f32, f32),
    OrbitStart(f32, f32),
    PinchStarted,
    PinchUpdated(f32, f32, f32),
    Resized(f32, f32),
    PickBrush,
    ProjectionSelected(bool),
    SplitLineHere,
    PasteSectorToSelected,
    Weld,
    MergeLines,
    Dissolve,
    AlignX,
    AlignY,
    DistributeX,
    DistributeY,
    Straighten,
    RotateCw,
    RotateCcw,
    MirrorH,
    MirrorV,
    AddSector,
    MergeSectors,
    UnmergeSector,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Delete,
    HeightStep { raise: bool, opposite: bool },
    CycleGrid,
    ToggleOverlays,
    FlipLines,
    SplitIntersections,
    ZoomIn,
    ZoomOut,
    CancelOrClear,
    CommitOrClear,
    ResetView,
    ToolChanged(ToolKind),
    FillChanged(UiSectorFill),
    NgonSidesChosen(i32),
    ZoomSelected(f32),
    GridSelected(i32),
    SnapToggled,
    SnapVertexToggled,
    SnapLineToggled,
    AngleSnapToggled,
    HighlightUnenclosedToggled,
    ThingsSelected(SkillFilter),
    LineApply,
    LineFlip,
    ThingApply,
    SectorApply,
    SectorNewSector,
    DrawApply,
}
pub(crate) enum MapRequest {
    Dialog(DialogRequest),
    Launch,
}
pub(crate) struct MapEffects {
    pub(crate) damage: Damage,
    pub(crate) request: Option<MapRequest>,
}
impl MapPresentation {
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive mapping keeps map command behavior visible and shared with headless callers"
    )]
    #[expect(
        clippy::cast_precision_loss,
        reason = "geometry operations use floating point Doom units"
    )]
    pub(crate) fn command(&mut self, state: &mut EditorSession, command: MapCommand) -> MapEffects {
        let mut request = None;
        let app = &mut state.map_editor;
        let damage = match command {
            MapCommand::ToolClick(x, y, shift) => {
                if state.bsp_anim.take().is_some() {
                    state.wgpu.set_overlay(&[], &[]);
                    Damage::Repaint
                } else {
                    let damage = app.tool_click([x, y], shift);
                    if app.tool == Tool::Launch {
                        request = Some(MapRequest::Launch);
                    }
                    preview::handle_pick(self, state, [x, y]);
                    damage
                }
            }
            MapCommand::ContextMenuOpening(x, y) => {
                app.cursor_world = app.screen_to_world([x, y]);
                Damage::None
            }
            MapCommand::ToolDragStart(x, y, shift) => app.begin_tool_drag([x, y], shift),
            MapCommand::ToolDrag(x, y) => app.drag_to([x, y]),
            MapCommand::ToolDragEnd(x, y) => {
                let damage = app.end_drag([x, y]);
                if app.line_drag_pending().is_some() {
                    self.canvas.line_drag_x = x;
                    self.canvas.line_drag_y = y;
                    self.canvas.line_drag_choice = 0;
                    self.canvas.line_drag_pending = true;
                }
                damage
            }
            MapCommand::LineDragMove => {
                self.canvas.line_drag_pending = false;
                app.commit_pending_move()
            }
            MapCommand::LineDragExtrude => {
                self.canvas.line_drag_pending = false;
                app.commit_pending_extrude()
            }
            MapCommand::LineDragCancel => {
                self.canvas.line_drag_pending = false;
                app.cancel_pending_drag()
            }
            MapCommand::Fillet(radius, segments) => {
                app.fillet_selected(radius as f32, segments.max(1).unsigned_abs())
            }
            MapCommand::Chamfer(dist) => app.chamfer_selected(dist as f32),
            MapCommand::Hover(x, y) => {
                let shape = app.shape_hover([x, y]);
                let damage = if shape == Damage::None {
                    app.hover_poly([x, y])
                } else {
                    shape
                };
                preview::handle_hover(self, state, [x, y]);
                damage
            }
            MapCommand::DoubleClicked(x, y) => {
                if matches!(app.tool, Tool::Select(_)) && app.map.is_some() {
                    if let Some(PickKind::Linedef(key)) = app
                        .pick_3d_select([x, y], SelectMode::Line)
                        .map(|hit| hit.kind)
                    {
                        request = Some(MapRequest::Dialog(DialogRequest::Wall(
                            i32::try_from(key.slot()).unwrap_or(-1),
                        )));
                    } else if let Some(PickKind::Sector(key)) = app
                        .pick_3d_select([x, y], SelectMode::Sector)
                        .map(|hit| hit.kind)
                    {
                        request = Some(MapRequest::Dialog(DialogRequest::Sector(
                            i32::try_from(key.slot()).unwrap_or(-1),
                        )));
                    }
                }
                Damage::None
            }
            MapCommand::Pan(x, y) => app.pan(x, y),
            MapCommand::ZoomAt(delta, x, y) => app.scroll_zoom(delta, [x, y]),
            MapCommand::Orbit(x, y) => app.orbit(x, y),
            MapCommand::OrbitStart(x, y) => {
                app.orbit_start([x, y]);
                Damage::None
            }
            MapCommand::PinchStarted => {
                state.pinch_scale = 1.;
                Damage::None
            }
            MapCommand::PinchUpdated(cumulative, x, y) => {
                let factor = if state.pinch_scale > 0. {
                    cumulative / state.pinch_scale
                } else {
                    1.
                };
                state.pinch_scale = cumulative;
                app.handle_pinch(factor, [x, y])
            }
            MapCommand::Resized(w, h) => app.set_viewport(w, h),
            MapCommand::PickBrush => {
                app.pick_brush_from_selection();
                Damage::None
            }
            MapCommand::ProjectionSelected(perspective) => app.set_projection(if perspective {
                Projection::Perspective
            } else {
                Projection::Ortho
            }),
            MapCommand::SplitLineHere => app.split_selected_line_at(app.cursor_world),
            MapCommand::PasteSectorToSelected | MapCommand::Paste => app.paste(),
            MapCommand::Weld => app.weld_selected(),
            MapCommand::MergeLines => app.merge_selected_lines(),
            MapCommand::Dissolve => app.dissolve_selected(),
            MapCommand::AlignX => app.align_selected(Axis::X),
            MapCommand::AlignY => app.align_selected(Axis::Y),
            MapCommand::DistributeX => app.distribute_selected(Axis::X),
            MapCommand::DistributeY => app.distribute_selected(Axis::Y),
            MapCommand::Straighten => app.straighten_selected(),
            MapCommand::RotateCw => app.rotate_selected_90(true),
            MapCommand::RotateCcw => app.rotate_selected_90(false),
            MapCommand::MirrorH => app.mirror_selected(Axis::X),
            MapCommand::MirrorV => app.mirror_selected(Axis::Y),
            MapCommand::AddSector => app.add_sector_at(app.cursor_world),
            MapCommand::MergeSectors => app.merge_selected_sectors(),
            MapCommand::UnmergeSector => app.unmerge_sector(app.cursor_world),
            MapCommand::Undo => app.undo(),
            MapCommand::Redo => app.redo(),
            MapCommand::Copy => app.copy_selection(),
            MapCommand::Cut => app.cut_selection(),
            MapCommand::Delete => app.delete_selection(),
            MapCommand::HeightStep {
                raise,
                opposite,
            } => app.nudge_selected_height(raise, opposite),
            MapCommand::CycleGrid => app.cycle_grid(),
            MapCommand::ToggleOverlays => app.toggle_overlays(),
            MapCommand::FlipLines => {
                let selected = app.selected_lines();
                app.flip_selected_lines(&selected)
            }
            MapCommand::SplitIntersections => app.split_selected_at_intersections(),
            MapCommand::ZoomIn => app.zoom_in(),
            MapCommand::ZoomOut => app.zoom_out(),
            MapCommand::CancelOrClear | MapCommand::CommitOrClear => {
                let dismissed_bsp = state.bsp_anim.take().is_some();
                state.wgpu.set_overlay(&[], &[]);
                let damage = if matches!(command, MapCommand::CancelOrClear) {
                    if app.drawing_active() {
                        app.discard_gesture()
                    } else {
                        app.clear_selection()
                    }
                } else {
                    app.cancel_gesture().combine(app.clear_selection())
                };
                if dismissed_bsp {
                    damage.combine(Damage::Repaint)
                } else {
                    damage
                }
            }
            MapCommand::ResetView => app.reset_view_top_down(),
            MapCommand::ToolChanged(tool) => {
                app.set_tool(tool.into());
                self.toolbar.current = tool;
                match tool {
                    ToolKind::Select
                    | ToolKind::SelectVertex
                    | ToolKind::SelectLine
                    | ToolKind::SelectThing
                    | ToolKind::SelectSector => self.toolbar.last_select = tool,
                    ToolKind::Line | ToolKind::Rect | ToolKind::Triangle | ToolKind::Ngon => {
                        self.toolbar.last_shape = tool;
                    }
                    _ => {}
                }
                Damage::Repaint
            }
            MapCommand::FillChanged(fill) => {
                app.sector_fill = SectorFill::from(fill);
                self.toolbar.sector_fill = fill;
                Damage::Edited
            }
            MapCommand::NgonSidesChosen(sides) => {
                let sides = sides.clamp(3, 32);
                app.set_ngon_sides(sides.unsigned_abs());
                Damage::None
            }
            MapCommand::ZoomSelected(level) => {
                if level <= 0. {
                    app.zoom_fit()
                } else {
                    app.zoom_to(level)
                }
            }
            MapCommand::GridSelected(grid) => app.set_grid(grid),
            MapCommand::SnapToggled => app.set_snap(!app.snap),
            MapCommand::SnapVertexToggled => {
                app.set_snap_to_vertex(!app.snap_to_vertex);
                Damage::None
            }
            MapCommand::SnapLineToggled => {
                app.set_snap_to_line(!app.snap_to_line);
                Damage::None
            }
            MapCommand::AngleSnapToggled => {
                app.set_angle_snap(!app.angle_snap);
                Damage::None
            }
            MapCommand::HighlightUnenclosedToggled => {
                app.set_highlight_unenclosed(!app.highlight_unenclosed)
            }
            MapCommand::ThingsSelected(filter) => app.set_skill_filter(filter.into()),
            MapCommand::LineApply => inspectors::line_apply(self, state),
            MapCommand::LineFlip => inspectors::line_flip(state),
            MapCommand::ThingApply => inspectors::thing_apply(self, state),
            MapCommand::SectorApply => inspectors::sector_apply(self, state),
            MapCommand::SectorNewSector => {
                inspectors::sector_new_sector(self, state);
                Damage::Edited
            }
            MapCommand::DrawApply => {
                draw::apply(self, state);
                Damage::None
            }
        };
        if matches!(
            command,
            MapCommand::SnapVertexToggled
                | MapCommand::SnapLineToggled
                | MapCommand::AngleSnapToggled
        ) {
            save_prefs_now(state);
        }
        if matches!(damage, Damage::Edited | Damage::Restyle) {
            self.panels_key = None;
        }
        if matches!(
            command,
            MapCommand::Hover(..)
                | MapCommand::Pan(..)
                | MapCommand::ZoomAt(..)
                | MapCommand::Orbit(..)
                | MapCommand::OrbitStart(..)
                | MapCommand::PinchStarted
                | MapCommand::PinchUpdated(..)
                | MapCommand::Resized(..)
                | MapCommand::ZoomSelected(..)
        ) {
            super::status::update_status(self, state);
        } else {
            self.sync(state);
        }
        MapEffects {
            damage,
            request,
        }
    }
    pub(crate) fn sync(&mut self, state: &mut EditorSession) {
        if state.map_editor.sampled_sector.take().is_some() {
            self.panels_key = None;
        }
        super::status::update_status(self, state);
        let app = &state.map_editor;
        self.toolbar.projection_perspective = app.projection() == Projection::Perspective;
        inspectors::sync(self, state);
    }
    pub(crate) fn asset_request(&self, target: TexBrowseTarget) -> DialogRequest {
        let current = match target {
            TexBrowseTarget::LineSlot(slot) => {
                let side = match slot {
                    TexSlot::BackTop | TexSlot::BackMid | TexSlot::BackBottom => &self.line.back,
                    _ => &self.line.front,
                };
                Some(match slot {
                    TexSlot::FrontTop | TexSlot::BackTop => side.top.clone(),
                    TexSlot::FrontBottom | TexSlot::BackBottom => side.bottom.clone(),
                    _ => side.mid.clone(),
                })
            }
            TexBrowseTarget::SectorSlot(FlatSlot::Floor) => Some(self.sector.floor_flat.clone()),
            TexBrowseTarget::SectorSlot(FlatSlot::Ceil) => Some(self.sector.ceil_flat.clone()),
            TexBrowseTarget::Brush => Some(self.draw_settings.wall_tex.clone()),
            TexBrowseTarget::DrawFloor => Some(self.draw_settings.floor_flat.clone()),
            TexBrowseTarget::DrawCeil => Some(self.draw_settings.ceil_flat.clone()),
            TexBrowseTarget::Patch => None,
        };
        DialogRequest::AssetPicker {
            target,
            current,
        }
    }
}
