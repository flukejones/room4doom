use crate::{CRT_HEIGHT, OG_HEIGHT, OG_WIDTH};

/// Artwork scales with Doom's vertical CRT correction, independent of placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiLayout {
    pub scale: [f32; 2],
    pub origin: [f32; 2],
    screen: [f32; 2],
}

/// A placement rectangle in buffer pixels. It supplies anchors and text-flow
/// bounds; drawing is clipped only by the actual buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiViewport {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub scale: [f32; 2],
}

impl UiViewport {
    pub fn point(self, x: f32, y: f32) -> [f32; 2] {
        [
            self.origin[0] + x * self.scale[0],
            self.origin[1] + y * self.scale[1],
        ]
    }

    pub fn right(self) -> f32 {
        self.origin[0] + self.size[0]
    }

    pub fn bottom(self) -> f32 {
        self.origin[1] + self.size[1]
    }
}

impl UiLayout {
    pub const DISPLAY_ASPECT: f32 = OG_WIDTH / CRT_HEIGHT;

    #[expect(
        clippy::cast_precision_loss,
        reason = "physical target sizes are converted to rendering coordinates"
    )]
    pub fn from_sizes(buffer: (u32, u32), display: (u32, u32)) -> Self {
        Self::new(
            [buffer.0 as f32, buffer.1 as f32],
            [display.0 as f32, display.1 as f32],
        )
    }

    /// Account for the stretch from the raster buffer to its displayed rectangle.
    pub fn new(buffer: [f32; 2], display: [f32; 2]) -> Self {
        let [width, height] = display.map(|v| v.max(1.0));
        let fit = (width / OG_WIDTH).min(height / CRT_HEIGHT);
        let content = [
            fit * OG_WIDTH * buffer[0] / width,
            fit * CRT_HEIGHT * buffer[1] / height,
        ];
        let scale = [content[0] / OG_WIDTH, content[1] / OG_HEIGHT];
        Self {
            scale,
            screen: buffer,
            origin: [
                (buffer[0] - content[0]) * 0.5,
                (buffer[1] - content[1]) * 0.5,
            ],
        }
    }

    pub fn width(self) -> f32 {
        OG_WIDTH * self.scale[0]
    }

    pub fn height(self) -> f32 {
        OG_HEIGHT * self.scale[1]
    }

    /// Classic artwork and menu coordinates, centered in the display.
    pub fn centered(self) -> UiViewport {
        UiViewport {
            origin: self.origin,
            size: [self.width(), self.height()],
            scale: self.scale,
        }
    }

    /// Full-buffer placement with the same corrected artwork scales.
    pub fn screen(self) -> UiViewport {
        UiViewport {
            origin: [0.0; 2],
            size: self.screen,
            scale: self.scale,
        }
    }

    /// HUDs use the screen's top/bottom and either classic or full-width anchors.
    pub fn hud(self, widescreen: bool) -> UiViewport {
        let mut viewport = self.screen();
        if !widescreen {
            viewport.origin[0] = self.origin[0];
            viewport.size[0] = self.width();
        }
        viewport
    }

    /// The classic statusbar's artwork remains centered and sits at screen bottom.
    pub fn statusbar(self) -> UiViewport {
        let mut viewport = self.centered();
        viewport.origin[1] = self.screen[1] - viewport.size[1];
        viewport
    }
}

#[cfg(test)]
mod tests {
    use super::UiLayout;

    #[test]
    fn crt_aspect_survives_native_and_classic_presentation() {
        for (buffer, display, expected) in [
            ([640.0, 480.0], [640.0, 480.0], [0.0, 0.0, 640.0, 480.0]),
            (
                [1920.0, 1080.0],
                [1920.0, 1080.0],
                [240.0, 0.0, 1440.0, 1080.0],
            ),
            ([320.0, 200.0], [640.0, 480.0], [0.0, 0.0, 320.0, 200.0]),
            ([640.0, 400.0], [640.0, 480.0], [0.0, 0.0, 640.0, 400.0]),
            ([480.0, 640.0], [480.0, 640.0], [0.0, 140.0, 480.0, 360.0]),
        ] {
            let layout = UiLayout::new(buffer, display);
            let actual = [
                layout.origin[0],
                layout.origin[1],
                layout.width(),
                layout.height(),
            ];
            for (a, e) in actual.into_iter().zip(expected) {
                assert!((a - e).abs() < 0.001, "{actual:?} != {expected:?}");
            }
            let physical_width = layout.width() * display[0] / buffer[0];
            let physical_height = layout.height() * display[1] / buffer[1];
            assert!((physical_width / physical_height - 4.0 / 3.0).abs() < 0.001);
        }
    }
}
