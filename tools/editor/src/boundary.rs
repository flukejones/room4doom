//! Domain types that cross the Slint boundary, with `From` conversions to generated mirrors.

use editor_core::ThingFlags;

use crate::generated;
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

impl From<Tool> for generated::ToolKind {
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

impl From<generated::ToolKind> for Tool {
    fn from(t: generated::ToolKind) -> Self {
        match t {
            generated::ToolKind::Select => Self::Select(SelectMode::All),
            generated::ToolKind::SelectVertex => Self::Select(SelectMode::Vertex),
            generated::ToolKind::SelectLine => Self::Select(SelectMode::Line),
            generated::ToolKind::SelectThing => Self::Select(SelectMode::Thing),
            generated::ToolKind::SelectSector => Self::Select(SelectMode::Sector),
            generated::ToolKind::Line => Self::Draw(DrawShape::Line),
            generated::ToolKind::Rect => Self::Draw(DrawShape::Rect),
            generated::ToolKind::Triangle => Self::Draw(DrawShape::Triangle),
            generated::ToolKind::Ngon => Self::Draw(DrawShape::Ngon),
            generated::ToolKind::Thing => Self::Thing,
            generated::ToolKind::Sector => Self::Sector,
            generated::ToolKind::Launch => Self::Launch,
        }
    }
}

impl From<SectorFill> for generated::SectorFill {
    fn from(f: SectorFill) -> Self {
        match f {
            SectorFill::None => Self::None,
            SectorFill::Colour => Self::Colour,
            SectorFill::Texture => Self::Texture,
        }
    }
}

impl From<generated::SectorFill> for SectorFill {
    fn from(f: generated::SectorFill) -> Self {
        match f {
            generated::SectorFill::None => Self::None,
            generated::SectorFill::Colour => Self::Colour,
            generated::SectorFill::Texture => Self::Texture,
        }
    }
}

impl From<SkillFilter> for generated::SkillFilter {
    fn from(s: SkillFilter) -> Self {
        match s {
            SkillFilter::All => Self::All,
            SkillFilter::Easy => Self::Easy,
            SkillFilter::Normal => Self::Normal,
            SkillFilter::Hard => Self::Hard,
        }
    }
}

impl From<generated::SkillFilter> for SkillFilter {
    fn from(s: generated::SkillFilter) -> Self {
        match s {
            generated::SkillFilter::All => Self::All,
            generated::SkillFilter::Easy => Self::Easy,
            generated::SkillFilter::Normal => Self::Normal,
            generated::SkillFilter::Hard => Self::Hard,
        }
    }
}

impl From<TexSlot> for generated::TexSlot {
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

impl From<generated::TexSlot> for TexSlot {
    fn from(s: generated::TexSlot) -> Self {
        match s {
            generated::TexSlot::FrontTop => Self::FrontTop,
            generated::TexSlot::FrontMid => Self::FrontMid,
            generated::TexSlot::FrontBottom => Self::FrontBottom,
            generated::TexSlot::BackTop => Self::BackTop,
            generated::TexSlot::BackMid => Self::BackMid,
            generated::TexSlot::BackBottom => Self::BackBottom,
        }
    }
}

impl From<FlatSlot> for generated::FlatSlot {
    fn from(s: FlatSlot) -> Self {
        match s {
            FlatSlot::Floor => Self::Floor,
            FlatSlot::Ceil => Self::Ceil,
        }
    }
}

impl From<generated::FlatSlot> for FlatSlot {
    fn from(s: generated::FlatSlot) -> Self {
        match s {
            generated::FlatSlot::Floor => Self::Floor,
            generated::FlatSlot::Ceil => Self::Ceil,
        }
    }
}
