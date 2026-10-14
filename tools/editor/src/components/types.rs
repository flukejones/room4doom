//! Typed values exchanged between editor domain callbacks and native views.

use crate::gfx::image::UiImage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolKind {
    #[default]
    Select,
    SelectVertex,
    SelectLine,
    SelectThing,
    SelectSector,
    Line,
    Rect,
    Triangle,
    Ngon,
    Thing,
    Sector,
    Launch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SectorFill {
    #[default]
    None,
    Colour,
    Texture,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkillFilter {
    #[default]
    All,
    Easy,
    Normal,
    Hard,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TexSlot {
    #[default]
    FrontTop,
    FrontMid,
    FrontBottom,
    BackTop,
    BackMid,
    BackBottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlatSlot {
    #[default]
    Floor,
    Ceil,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupId {
    #[default]
    MapList,
    Browser,
    Remap,
    Prefs,
    WallEdit,
    SectorEdit,
    ProjectSettings,
    BuildBsp,
    NewMap,
    Audit,
    Animations,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GfxEntry {
    pub name: String,
    pub thumb: UiImage,
    pub tex_w: i32,
    pub tex_h: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PopupGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineFlags {
    pub blocks: bool,
    pub block_monsters: bool,
    pub two_sided: bool,
    pub upper_unpeg: bool,
    pub lower_unpeg: bool,
    pub secret: bool,
    pub block_sound: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineSide {
    pub top: String,
    pub mid: String,
    pub bottom: String,
    pub xoff: String,
    pub yoff: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineTouched {
    pub blocks: bool,
    pub block_monsters: bool,
    pub two_sided: bool,
    pub upper_unpeg: bool,
    pub lower_unpeg: bool,
    pub secret: bool,
    pub block_sound: bool,
    pub hidden: bool,
    pub special: bool,
    pub tag: bool,
    pub front_top: bool,
    pub front_mid: bool,
    pub front_bottom: bool,
    pub front_xoff: bool,
    pub front_yoff: bool,
    pub back_top: bool,
    pub back_mid: bool,
    pub back_bottom: bool,
    pub back_xoff: bool,
    pub back_yoff: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemapRow {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemapKind {
    #[default]
    Thing,
    Texture,
    Flat,
    LineSpecial,
    SectorSpecial,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThingOptions {
    pub easy: bool,
    pub normal: bool,
    pub hard: bool,
    pub ambush: bool,
    pub multi: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThingTouched {
    pub r#type: bool,
    pub angle: bool,
    pub easy: bool,
    pub normal: bool,
    pub hard: bool,
    pub ambush: bool,
    pub multi: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallSide {
    pub visible: bool,
    pub img: UiImage,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallPreview {
    pub visible: bool,
    pub pinned: bool,
    pub x: f32,
    pub y: f32,
    pub front: WallSide,
    pub back: WallSide,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SectorPreview {
    pub visible: bool,
    pub ceil_img: UiImage,
    pub floor_img: UiImage,
    pub side: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BandRect {
    pub y: f32,
    pub h: f32,
    pub slot: TexSlot,
}
