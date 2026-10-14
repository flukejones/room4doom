//! Domain types that cross the GPUI boundary, with `From` conversions to native presentation values.

use editor_core::ThingFlags;

use crate::components::types as ui_model;
use crate::render::input::SectorFill;

/// Draw-tool shape. `Line` = freeform chain; others = two-click. N-gon sides in `ngon_sides`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawShape {
    #[default]
    Line,
    Rect,
    Triangle,
    Ngon,
}

/// Select-tool filter. `All` = full priority order; others restrict to one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMode {
    #[default]
    All,
    Vertex,
    Line,
    Thing,
    Sector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select(SelectMode),
    /// Line/shape draw; closed shape or chain derives a sector.
    Draw(DrawShape),
    Thing,
    Sector,
    Launch,
}

impl Default for Tool {
    fn default() -> Self {
        Self::Select(SelectMode::All)
    }
}

/// Canvas thing filter by difficulty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillFilter {
    #[default]
    All,
    Easy,
    Normal,
    Hard,
}

impl SkillFilter {
    pub fn allows(self, options: ThingFlags) -> bool {
        let bit = match self {
            Self::All => return true,
            Self::Easy => ThingFlags::EASY,
            Self::Normal => ThingFlags::NORMAL,
            Self::Hard => ThingFlags::HARD,
        };
        options.contains(bit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexSlot {
    FrontTop,
    FrontMid,
    FrontBottom,
    BackTop,
    BackMid,
    BackBottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatSlot {
    Floor,
    Ceil,
}

impl From<Tool> for ui_model::ToolKind {
    fn from(t: Tool) -> Self {
        match t {
            Tool::Select(SelectMode::All) => Self::Select,
            Tool::Select(SelectMode::Vertex) => Self::SelectVertex,
            Tool::Select(SelectMode::Line) => Self::SelectLine,
            Tool::Select(SelectMode::Thing) => Self::SelectThing,
            Tool::Select(SelectMode::Sector) => Self::SelectSector,
            Tool::Draw(DrawShape::Line) => Self::Line,
            Tool::Draw(DrawShape::Rect) => Self::Rect,
            Tool::Draw(DrawShape::Triangle) => Self::Triangle,
            Tool::Draw(DrawShape::Ngon) => Self::Ngon,
            Tool::Thing => Self::Thing,
            Tool::Sector => Self::Sector,
            Tool::Launch => Self::Launch,
        }
    }
}

impl From<ui_model::ToolKind> for Tool {
    fn from(t: ui_model::ToolKind) -> Self {
        match t {
            ui_model::ToolKind::Select => Self::Select(SelectMode::All),
            ui_model::ToolKind::SelectVertex => Self::Select(SelectMode::Vertex),
            ui_model::ToolKind::SelectLine => Self::Select(SelectMode::Line),
            ui_model::ToolKind::SelectThing => Self::Select(SelectMode::Thing),
            ui_model::ToolKind::SelectSector => Self::Select(SelectMode::Sector),
            ui_model::ToolKind::Line => Self::Draw(DrawShape::Line),
            ui_model::ToolKind::Rect => Self::Draw(DrawShape::Rect),
            ui_model::ToolKind::Triangle => Self::Draw(DrawShape::Triangle),
            ui_model::ToolKind::Ngon => Self::Draw(DrawShape::Ngon),
            ui_model::ToolKind::Thing => Self::Thing,
            ui_model::ToolKind::Sector => Self::Sector,
            ui_model::ToolKind::Launch => Self::Launch,
        }
    }
}

impl From<SectorFill> for ui_model::SectorFill {
    fn from(f: SectorFill) -> Self {
        match f {
            SectorFill::None => Self::None,
            SectorFill::Colour => Self::Colour,
            SectorFill::Texture => Self::Texture,
        }
    }
}

impl From<ui_model::SectorFill> for SectorFill {
    fn from(f: ui_model::SectorFill) -> Self {
        match f {
            ui_model::SectorFill::None => Self::None,
            ui_model::SectorFill::Colour => Self::Colour,
            ui_model::SectorFill::Texture => Self::Texture,
        }
    }
}

impl From<SkillFilter> for ui_model::SkillFilter {
    fn from(s: SkillFilter) -> Self {
        match s {
            SkillFilter::All => Self::All,
            SkillFilter::Easy => Self::Easy,
            SkillFilter::Normal => Self::Normal,
            SkillFilter::Hard => Self::Hard,
        }
    }
}

impl From<ui_model::SkillFilter> for SkillFilter {
    fn from(s: ui_model::SkillFilter) -> Self {
        match s {
            ui_model::SkillFilter::All => Self::All,
            ui_model::SkillFilter::Easy => Self::Easy,
            ui_model::SkillFilter::Normal => Self::Normal,
            ui_model::SkillFilter::Hard => Self::Hard,
        }
    }
}

impl From<TexSlot> for ui_model::TexSlot {
    fn from(s: TexSlot) -> Self {
        match s {
            TexSlot::FrontTop => Self::FrontTop,
            TexSlot::FrontMid => Self::FrontMid,
            TexSlot::FrontBottom => Self::FrontBottom,
            TexSlot::BackTop => Self::BackTop,
            TexSlot::BackMid => Self::BackMid,
            TexSlot::BackBottom => Self::BackBottom,
        }
    }
}

impl From<ui_model::TexSlot> for TexSlot {
    fn from(s: ui_model::TexSlot) -> Self {
        match s {
            ui_model::TexSlot::FrontTop => Self::FrontTop,
            ui_model::TexSlot::FrontMid => Self::FrontMid,
            ui_model::TexSlot::FrontBottom => Self::FrontBottom,
            ui_model::TexSlot::BackTop => Self::BackTop,
            ui_model::TexSlot::BackMid => Self::BackMid,
            ui_model::TexSlot::BackBottom => Self::BackBottom,
        }
    }
}

impl From<FlatSlot> for ui_model::FlatSlot {
    fn from(s: FlatSlot) -> Self {
        match s {
            FlatSlot::Floor => Self::Floor,
            FlatSlot::Ceil => Self::Ceil,
        }
    }
}

impl From<ui_model::FlatSlot> for FlatSlot {
    fn from(s: ui_model::FlatSlot) -> Self {
        match s {
            ui_model::FlatSlot::Floor => Self::Floor,
            ui_model::FlatSlot::Ceil => Self::Ceil,
        }
    }
}
