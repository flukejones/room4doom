#[cfg(feature = "software25d")]
use render_common::CRT_STRETCH;

use crate::{RenderKind, RenderType};
use render_common::UiLayout;

const CLASSIC_HEIGHT: u32 = 200;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResolutionOptions {
    pub hi_res: bool,
    pub native_ui: bool,
    pub native_world: bool,
}

impl From<bool> for ResolutionOptions {
    fn from(hi_res: bool) -> Self {
        Self {
            hi_res,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Resolution {
    pub world: (u32, u32),
    pub ui: (u32, u32),
    pub output: (u32, u32),
    pub display: (u32, u32),
}

impl Resolution {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "render targets quantize physical dimensions"
    )]
    pub fn new(
        options: ResolutionOptions,
        renderer: RenderType,
        display: (u32, u32),
        aspect: f32,
        stereo: bool,
    ) -> Self {
        let display = (display.0.max(1), display.1.max(1));
        let height = CLASSIC_HEIGHT * if options.hi_res { 2 } else { 1 };
        #[cfg(feature = "software25d")]
        let pixel_aspect = if renderer == RenderType::Software {
            CRT_STRETCH
        } else {
            1.0
        };
        #[cfg(not(feature = "software25d"))]
        let pixel_aspect = 1.0;
        let classic = (
            ((height as f32 * aspect * pixel_aspect).round() as u32).max(height),
            height,
        );
        let hardware = renderer.kind() == RenderKind::Hardware;
        let world = if hardware && options.native_world {
            display
        } else {
            classic
        };
        let native_ui = options.native_ui || stereo;
        let ui = if stereo {
            (
                ((display.1 as f32 * UiLayout::DISPLAY_ASPECT).round() as u32).max(1),
                display.1,
            )
        } else if native_ui {
            display
        } else {
            classic
        };
        let output = if native_ui || (hardware && options.native_world) {
            display
        } else {
            classic
        };
        Self {
            world,
            ui,
            output,
            display,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "software3d")]
    #[test]
    fn software_keeps_classic_world_with_native_ui() {
        let options = ResolutionOptions {
            hi_res: false,
            native_ui: true,
            native_world: true,
        };
        let sizes = Resolution::new(
            options,
            RenderType::Software3D,
            (1920, 1080),
            16.0 / 9.0,
            false,
        );
        assert_eq!(sizes.world, (356, 200));
        assert_eq!(sizes.ui, (1920, 1080));
        assert_eq!(sizes.output, (1920, 1080));
    }

    #[cfg(feature = "wgpu3d")]
    #[test]
    fn hardware_resolutions_are_independent_and_vr_forces_ui() {
        for native_ui in [false, true] {
            for native_world in [false, true] {
                let options = ResolutionOptions {
                    hi_res: false,
                    native_ui,
                    native_world,
                };
                let sizes =
                    Resolution::new(options, RenderType::Wgpu3D, (1920, 1080), 16.0 / 9.0, false);
                assert_eq!(
                    sizes.world,
                    if native_world {
                        (1920, 1080)
                    } else {
                        (356, 200)
                    }
                );
                assert_eq!(sizes.ui, if native_ui { (1920, 1080) } else { (356, 200) });
                assert_eq!(
                    sizes.output,
                    if native_ui || native_world {
                        (1920, 1080)
                    } else {
                        (356, 200)
                    }
                );
                let vr = Resolution::new(options, RenderType::Wgpu3D, (1800, 2000), 1.1, true);
                assert_eq!(
                    vr.world,
                    if native_world {
                        (1800, 2000)
                    } else {
                        (220, 200)
                    }
                );
                assert_eq!(vr.ui, (2667, 2000));
                assert_eq!(vr.output, (1800, 2000));
            }
        }
    }
}
