//! Small application-owned vector icons. No font glyphs or external icon assets.

use crate::widgets::Color;
use gpui::{App, IntoElement, PathBuilder, RenderOnce, Window, canvas, point, prelude::*, px};

#[derive(Clone, Copy)]
pub enum IconName {
    ChevronDown,
    ChevronRight,
    Box,
    BoxOpen,
    Image,
    RotateCw,
    EditorCursor,
    Crosshair,
    Dash,
    Person,
    SquareDot,
    Triangle,
    SelectAll,
    Plus,
    PlayFilled,
    Trash,
    FileGeneric,
    Folder,
    Check,
}

#[derive(Clone, Copy)]
pub enum IconSize {
    XSmall,
    Small,
}

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: f32,
    color: Color,
}
impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: 16.,
            color: Color::Default,
        }
    }
    pub fn size(mut self, size: IconSize) -> Self {
        self.size = match size {
            IconSize::XSmall => 12.,
            IconSize::Small => 16.,
        };
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Icon {
    fn paint(self, bounds: gpui::Bounds<gpui::Pixels>, window: &mut Window, cx: &App) {
        let mut builder = PathBuilder::stroke(px(1.5));
        let mut stroke = |points: &[[f32; 2]]| {
            let p = |v: [f32; 2]| {
                point(
                    bounds.left() + bounds.size.width * (v[0] / 24.),
                    bounds.top() + bounds.size.height * (v[1] / 24.),
                )
            };
            if let Some((&first, rest)) = points.split_first() {
                builder.move_to(p(first));
                for &v in rest {
                    builder.line_to(p(v));
                }
            }
        };
        match self.name {
            IconName::ChevronDown => stroke(&[[6., 9.], [12., 15.], [18., 9.]]),
            IconName::ChevronRight => stroke(&[[9., 6.], [15., 12.], [9., 18.]]),
            IconName::Check => stroke(&[[4., 12.], [9., 17.], [20., 5.]]),
            IconName::Dash => stroke(&[[3., 12.], [21., 12.]]),
            IconName::Plus | IconName::Crosshair => {
                stroke(&[[3., 12.], [21., 12.]]);
                stroke(&[[12., 3.], [12., 21.]]);
            }
            IconName::Triangle | IconName::PlayFilled => {
                stroke(&[[5., 3.], [21., 12.], [5., 21.], [5., 3.]]);
            }
            IconName::EditorCursor => {
                stroke(&[[5., 3.], [19., 13.], [12., 14.], [9., 21.], [5., 3.]]);
            }
            IconName::RotateCw => {
                stroke(&[
                    [18., 7.],
                    [14., 3.],
                    [7., 4.],
                    [3., 10.],
                    [5., 18.],
                    [12., 21.],
                    [19., 17.],
                ]);
                stroke(&[[18., 2.], [18., 8.], [12., 8.]]);
            }
            IconName::Trash => {
                stroke(&[[3., 6.], [21., 6.]]);
                stroke(&[[8., 6.], [8., 3.], [16., 3.], [16., 6.]]);
                stroke(&[[6., 6.], [7., 21.], [17., 21.], [18., 6.]]);
            }
            IconName::Person => {
                stroke(&[[8., 8.], [8., 3.], [16., 3.], [16., 8.], [8., 8.]]);
                stroke(&[[4., 21.], [6., 12.], [18., 12.], [20., 21.]]);
            }
            IconName::Box | IconName::BoxOpen => {
                stroke(&[
                    [3., 7.],
                    [12., 2.],
                    [21., 7.],
                    [21., 17.],
                    [12., 22.],
                    [3., 17.],
                    [3., 7.],
                    [12., 12.],
                    [21., 7.],
                ]);
                stroke(&[[12., 12.], [12., 22.]]);
            }
            IconName::Folder => stroke(&[
                [2., 6.],
                [9., 6.],
                [12., 9.],
                [22., 9.],
                [22., 20.],
                [2., 20.],
                [2., 6.],
            ]),
            IconName::FileGeneric => stroke(&[
                [5., 2.],
                [14., 2.],
                [20., 8.],
                [20., 22.],
                [5., 22.],
                [5., 2.],
            ]),
            IconName::Image => {
                stroke(&[[3., 3.], [21., 3.], [21., 21.], [3., 21.], [3., 3.]]);
                stroke(&[[3., 18.], [10., 10.], [15., 15.], [18., 12.], [21., 16.]]);
            }
            IconName::SquareDot | IconName::SelectAll => {
                stroke(&[[3., 3.], [21., 3.], [21., 21.], [3., 21.], [3., 3.]]);
                stroke(&[[10., 12.], [14., 12.]]);
            }
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, self.color.resolve(cx));
        }
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let size = px(self.size);
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| self.paint(bounds, window, cx),
        )
        .w(size)
        .h(size)
        .flex_shrink_0()
    }
}
