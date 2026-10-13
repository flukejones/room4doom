//! CPU storage for composed software frames or independently placed GPU panels.

use pic_data::{ByteOrder, PixelFmt};

#[cfg(feature = "cpu-render")]
use crate::Frame;
#[cfg(feature = "wgpu3d")]
use crate::backend::{UiPixels, UiPlanes};
use crate::resolution::Resolution;
#[cfg(feature = "wgpu3d")]
use crate::wgpu_backend::as_u32_slice;
use crate::{RenderKind, UiSurface};

pub(crate) enum RenderTargets<P: PixelFmt> {
    #[cfg(feature = "cpu-render")]
    Software {
        frame: Frame<P>,
        scene: Option<UiSurface<P>>,
    },
    #[cfg(feature = "wgpu3d")]
    Hardware {
        hud: UiSurface<P>,
        screen: Option<UiSurface<P>>,
        open: bool,
    },
}

impl<P: PixelFmt> RenderTargets<P> {
    pub(crate) fn new(resolution: Resolution, kind: RenderKind, order: ByteOrder) -> Self {
        let (w, h) = resolution.ui;
        let mut targets = match kind {
            #[cfg(feature = "cpu-render")]
            RenderKind::Software => Self::Software {
                frame: Frame::new(w, h, order),
                scene: None,
            },
            #[cfg(feature = "wgpu3d")]
            RenderKind::Hardware => Self::Hardware {
                hud: UiSurface::new(w, h, order),
                screen: None,
                open: false,
            },
            #[allow(unreachable_patterns)]
            _ => unreachable!("render kind is not compiled"),
        };
        targets.reconfigure(resolution, kind, order);
        targets
    }

    pub(crate) fn reconfigure(
        &mut self,
        resolution: Resolution,
        kind: RenderKind,
        order: ByteOrder,
    ) {
        let (w, h) = resolution.ui;
        match self {
            #[cfg(feature = "cpu-render")]
            Self::Software {
                frame,
                scene,
            } if kind == RenderKind::Software => {
                frame.resize(w, h);
                frame.set_display_size(resolution.display);
                if resolution.world == resolution.ui {
                    *scene = None;
                } else {
                    let (sw, sh) = resolution.world;
                    scene
                        .get_or_insert_with(|| UiSurface::new(sw, sh, order))
                        .resize(sw, sh);
                }
            }
            #[cfg(feature = "wgpu3d")]
            Self::Hardware {
                hud,
                screen,
                open,
            } if kind == RenderKind::Hardware => {
                hud.resize(w, h);
                hud.set_display_size(if resolution.stereo {
                    resolution.ui
                } else {
                    resolution.display
                });
                if resolution.stereo {
                    screen
                        .get_or_insert_with(|| UiSurface::new(w, h, order))
                        .resize(w, h);
                } else {
                    *screen = None;
                }
                *open = false;
            }
            _ => *self = Self::new(resolution, kind, order),
        }
    }

    /// The first UI access clears GPU panels once; software UI overlays its scene.
    fn open_ui(&mut self) {
        match self {
            #[cfg(feature = "cpu-render")]
            Self::Software {
                ..
            } => {}
            #[cfg(feature = "wgpu3d")]
            Self::Hardware {
                hud,
                screen,
                open,
            } => {
                if !*open {
                    hud.clear_transparent();
                    if let Some(screen) = screen {
                        screen.clear_transparent();
                    }
                    *open = true;
                }
            }
        }
    }

    pub(crate) fn hud_mut(&mut self) -> &mut UiSurface<P> {
        self.open_ui();
        match self {
            #[cfg(feature = "cpu-render")]
            Self::Software {
                frame,
                ..
            } => frame.surface_mut(),
            #[cfg(feature = "wgpu3d")]
            Self::Hardware {
                hud,
                ..
            } => hud,
        }
    }

    pub(crate) fn screen_mut(&mut self) -> &mut UiSurface<P> {
        self.open_ui();
        match self {
            #[cfg(feature = "cpu-render")]
            Self::Software {
                frame,
                ..
            } => frame.surface_mut(),
            #[cfg(feature = "wgpu3d")]
            Self::Hardware {
                hud,
                screen,
                ..
            } => screen.as_mut().unwrap_or(hud),
        }
    }

    #[cfg(feature = "cpu-render")]
    pub(crate) fn software_frame(&self) -> &Frame<P> {
        match self {
            Self::Software {
                frame,
                ..
            } => frame,
            #[cfg(feature = "wgpu3d")]
            _ => unreachable!("software frame requested on a GPU renderer"),
        }
    }

    #[cfg(feature = "cpu-render")]
    pub(crate) fn software_mut(&mut self) -> (&mut Frame<P>, &mut Option<UiSurface<P>>) {
        match self {
            Self::Software {
                frame,
                scene,
            } => (frame, scene),
            #[cfg(feature = "wgpu3d")]
            _ => unreachable!("software frame requested on a GPU renderer"),
        }
    }

    #[cfg(feature = "wgpu3d")]
    pub(crate) fn ui_planes(&mut self) -> UiPlanes<'_> {
        fn pixels<P: PixelFmt>(surface: &UiSurface<P>) -> UiPixels<'_> {
            UiPixels {
                pixels: as_u32_slice(surface.pixels()),
                w: surface.width(),
                h: surface.height(),
            }
        }

        self.open_ui();
        match self {
            Self::Hardware {
                hud,
                screen,
                open,
            } => {
                *open = false;
                UiPlanes {
                    hud: hud.is_dirty().then(|| pixels(hud)),
                    screen: screen.as_ref().filter(|s| s.is_dirty()).map(pixels),
                }
            }
            #[cfg(feature = "cpu-render")]
            _ => unreachable!("GPU panels requested on a software renderer"),
        }
    }
}
