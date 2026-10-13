use crate::{CRT_HEIGHT, OG_HEIGHT, OG_WIDTH};

/// A centered 320×200 UI, displayed with Doom's 320×240 CRT aspect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiLayout {
    pub scale: [f32; 2],
    pub origin: [f32; 2],
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
