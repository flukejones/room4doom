//! Everything that exists only while a headset is attached, and the state
//! machine that runs its frame: open, per-eye, end.

use render_common::{EYE_COUNT, EyeView};

use crate::backend::PanelPlacement;

/// Index of each plane in [`Headset::panels`].
pub(crate) const HUD_PANEL: usize = 0;
pub(crate) const SCREEN_PANEL: usize = 1;

/// One field on the presenter rather than six, so the window path carries none
/// of it.
pub(crate) struct Headset {
    pub session: vr::Session,
    /// Eye the next scene pass targets and presents to.
    eye: usize,
    /// The runtime frame opened for this display frame.
    frame: Option<vr::FrameContext>,
    /// Pose of the eye being drawn, cached by [`Self::set_eye`].
    pub camera: Option<EyeView>,
    /// Panel geometry, indexed by [`HUD_PANEL`]/[`SCREEN_PANEL`].
    pub panels: [PanelPlacement; 2],
    /// Head yaw the facing-locked panel is pinned to.
    pub panel_yaw: f32,
}

impl Headset {
    /// Panel geometry starts here, until the config supplies its own.
    const DEFAULT_PANELS: [PanelPlacement; 2] = [
        PanelPlacement {
            distance_m: 1.2,
            width_deg: 45.0,
            pitch_deg: -15.0,
        },
        PanelPlacement {
            distance_m: 2.5,
            width_deg: 55.0,
            pitch_deg: 0.0,
        },
    ];

    pub fn new(session: vr::Session) -> Self {
        Self {
            session,
            eye: 0,
            frame: None,
            camera: None,
            panels: Self::DEFAULT_PANELS,
            panel_yaw: 0.0,
        }
    }

    /// Open the runtime's frame and locate both eyes. The runtime paces the
    /// display, so this is where a frame begins; nothing further should be
    /// drawn for this tic when it returns `None`.
    pub fn begin_frame(&mut self) -> Option<[Option<EyeView>; EYE_COUNT]> {
        self.camera = None;
        let frame = match self.session.begin_frame() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("vr: begin_frame: {e}");
                return None;
            }
        };
        // Widths must agree with `EYE_COUNT`, which the return type enforces.
        let views: [Option<EyeView>; vr::EYE_COUNT] = std::array::from_fn(|eye| {
            self.session
                .eye_target(eye, &frame)
                .map(|t| eye_view(&t.camera))
        });
        self.frame = Some(frame);
        Some(views)
    }

    /// Aim the next scene and present at `eye` of the frame already begun.
    pub fn set_eye(&mut self, eye: usize) {
        self.eye = eye;
        self.camera = self
            .frame
            .as_ref()
            .and_then(|frame| self.session.eye_target(eye, frame))
            .map(|t| eye_view(&t.camera));
    }

    pub fn eye(&self) -> usize {
        self.eye
    }

    /// Whether the runtime frame is open — presenting an eye needs one.
    pub fn frame_open(&self) -> bool {
        self.frame.is_some()
    }

    /// The current eye's swapchain image, when the runtime handed one out.
    pub fn eye_colour(&self) -> Option<&wgpu::TextureView> {
        let frame = self.frame.as_ref()?;
        Some(self.session.eye_target(self.eye, frame)?.colour)
    }

    /// After presenting the current eye: advance to the next, ending the
    /// runtime frame after the last.
    pub fn finish_eye(&mut self) {
        if self.eye + 1 >= vr::EYE_COUNT {
            if let Some(frame) = self.frame.take()
                && let Err(e) = self.session.end_frame(&frame)
            {
                log::warn!("vr: end_frame: {e}");
            }
            self.eye = 0;
        } else {
            self.eye += 1;
        }
    }

    /// Pin the facing-locked panel to the head yaw now, so a menu opens where
    /// the player is looking however far they have physically turned.
    pub fn anchor_panels(&mut self) {
        self.panel_yaw = self.session.head_yaw();
    }
}

/// The runtime's eye pose as the plain numbers the rest of the engine takes.
fn eye_view(camera: &vr::EyeCamera) -> EyeView {
    EyeView {
        playspace_yaw: camera.playspace_yaw,
        position: camera.position,
        orientation: camera.orientation,
        fov: camera.fov,
    }
}
