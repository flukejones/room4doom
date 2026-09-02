//! Session lifetime, swapchain, and the per-frame loop.

use std::ffi::c_void;
use std::fmt;

use log::{info, warn};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLDevice;
use openxr_sys as sys;
use openxr_sys::Handle as _;

use crate::EYE_COUNT;
use crate::actions::{Actions, ControllerState, SnapTurn};
use crate::device::{build_wgpu, wrap_texture};
use crate::hands::{HandJoints, Hands};
use crate::xr::{self, Instance, XrError};

/// Runtime-declared sRGB format; the renderer writes encoded pixels through an UNORM view.
const MTL_BGRA8_UNORM_SRGB: i64 = 81;
/// How long startup waits for the runtime to report a field of view.
const FOV_PRIME_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
const FOV_PRIME_POLL: std::time::Duration = std::time::Duration::from_millis(5);

#[derive(Debug)]
pub enum VrError {
    Xr(XrError),
    /// The runtime named a device Metal would not hand back.
    NoMetalDevice,
    /// The runtime returned a null image from `xrEnumerateSwapchainImages`.
    MissingSwapchainImage,
    /// A runtime image has a native format other than the requested BGRA8 sRGB.
    UnexpectedSwapchainImageFormat(usize),
    /// Metal could not create the compatible UNORM view used for encoded renderer output.
    SwapchainImageView {
        /// Native source pixel format rejected by `newTextureViewWithPixelFormat`.
        format: usize,
    },
    /// No wgpu Metal adapter matches the runtime-selected device's registry ID.
    NoMetalAdapter(u64),
    /// The native Metal adapter could not be opened.
    MetalAdapter(wgpu_hal::DeviceError),
    /// wgpu refused the device built from the runtime's `MTLDevice`.
    Device(wgpu::RequestDeviceError),
    /// The runtime asked the session to stop; no more frames may begin.
    Stopped,
}

impl fmt::Display for VrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xr(e) => write!(f, "{e}"),
            Self::NoMetalDevice => write!(f, "runtime returned no MTLDevice"),
            Self::MissingSwapchainImage => {
                write!(
                    f,
                    "xrEnumerateSwapchainImages returned a null Metal texture"
                )
            }
            Self::UnexpectedSwapchainImageFormat(format) => write!(
                f,
                "runtime Metal image format is {format}, expected BGRA8Unorm_sRGB (81)"
            ),
            Self::SwapchainImageView {
                format,
            } => write!(
                f,
                "MTLTexture::newTextureViewWithPixelFormat(BGRA8Unorm): source format {format}"
            ),
            Self::NoMetalAdapter(id) => write!(f, "no wgpu Metal adapter for runtime device {id}"),
            Self::MetalAdapter(e) => write!(f, "opening Metal adapter: {e}"),
            Self::Device(e) => write!(f, "wgpu device from runtime MTLDevice: {e}"),
            Self::Stopped => write!(f, "session stopped by the runtime"),
        }
    }
}

impl From<XrError> for VrError {
    fn from(e: XrError) -> Self {
        Self::Xr(e)
    }
}

/// Where one eye is and what it can see, as plain data — the camera the
/// caller turns into whatever view type its renderer wants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeCamera {
    /// How the play space these poses live in is oriented in the world,
    /// radians, positive left. The view is built on this and the eye's
    /// own rotation composed on top, so the head stays at frame rate
    /// while the world only turns on a snap.
    pub playspace_yaw: f32,
    /// Eye position in metres, play space (right-handed, -Z forward,
    /// +Y up).
    pub position: [f32; 3],
    /// Eye orientation as an xyzw quaternion.
    pub orientation: [f32; 4],
    /// Tangent-space field of view: left, right, up, down in radians.
    pub fov: [f32; 4],
}

/// One eye's region in the combined swapchain image.
pub struct EyeTarget<'a> {
    /// Combined stereo image shared by both eyes.
    pub colour: &'a wgpu::TextureView,
    /// Eye region as x, y, width, and height.
    pub rect: [u32; 4],
    /// Eye pose and field of view for this frame.
    pub camera: EyeCamera,
}

struct Swapchain {
    handle: sys::Swapchain,
    /// One wgpu view per runtime image, indexed by the acquired index.
    views: Vec<wgpu::TextureView>,
    /// Image acquired for the current frame.
    acquired: Option<u32>,
}

/// A live VR session: instance, swapchain, and the wgpu device the
/// renderer must use.
pub struct Session {
    instance: Instance,
    handle: sys::Session,
    space: sys::Space,
    swapchain: Swapchain,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The instance the device came from. A surface must be created from
    /// this one or it cannot be configured with our device.
    wgpu_instance: wgpu::Instance,
    width: u32,
    height: u32,
    /// Set once the runtime says the session may render.
    running: bool,
    /// Tangent-space width:height of an eye's field of view, learned from
    /// the first located frame.
    fov_aspect: Option<f32>,
    /// Most recent views the runtime reported as valid; reused while the
    /// runtime reports invalid poses (tracking not settled or lost).
    last_views: Option<[sys::View; EYE_COUNT]>,
    /// Controller actions. `None` when the runtime refused them — the
    /// game keeps running on keyboard and mouse.
    actions: Option<Actions>,
    /// Latest controller reading, refreshed once per frame.
    controls: ControllerState,
    /// Bare-hand joints, when the runtime tracks them.
    hands: Option<Hands>,
    /// How the play space is oriented in the world, radians, positive
    /// left. Rotated by snap turns and by [`Self::align_playspace`].
    playspace_yaw: f32,
    /// Whether a UI plane is showing, which layers the menu action set over
    /// gameplay at the next sync.
    ui_active: bool,
}

/// Wraps an angle to (-pi, pi].
fn wrap_pi(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let a = a % t;
    if a > std::f32::consts::PI {
        a - t
    } else if a <= -std::f32::consts::PI {
        a + t
    } else {
        a
    }
}

impl Session {
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The instance our device belongs to — any surface a caller wants to
    /// present with that device must be created from this.
    pub fn wgpu_instance(&self) -> &wgpu::Instance {
        &self.wgpu_instance
    }

    /// What the player asked for in the frame just begun. All-zero with
    /// no controllers, or while the session is not focused.
    pub fn controls(&self) -> ControllerState {
        self.controls
    }

    /// Head yaw within the play space, radians, positive left.
    pub fn head_yaw(&self) -> f32 {
        let Some(views) = self.last_views.as_ref() else {
            return 0.0;
        };
        let q = views[0].pose.orientation;
        (2.0 * (q.w * q.y + q.x * q.z)).atan2(1.0 - 2.0 * (q.y * q.y + q.x * q.x))
    }

    /// Midpoint of the two eyes, metres in play space. Place anything meant to
    /// hang in front of the viewer from here — placed per eye instead, it has
    /// no disparity and sits at infinity.
    pub fn head_position(&self) -> [f32; 3] {
        let Some(views) = self.last_views.as_ref() else {
            return [0.0; 3];
        };
        let (a, b) = (views[0].pose.position, views[1].pose.position);
        [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5]
    }

    /// Whether a UI plane is showing, which layers the menu action set over
    /// gameplay at the next sync.
    pub fn set_ui_active(&mut self, active: bool) {
        self.ui_active = active;
    }

    /// Where the player is looking, in the world: the play space's own
    /// orientation plus the head's within it. The game keeps the player
    /// pointed here.
    pub fn look_yaw(&self) -> f32 {
        wrap_pi(self.playspace_yaw + self.head_yaw())
    }

    /// How the play space is oriented in the world, radians, positive
    /// left. The render builds its view on this and composes the head's
    /// rotation on top.
    pub fn playspace_yaw(&self) -> f32 {
        self.playspace_yaw
    }

    /// Rotates the play space so the current gaze lands on `look`
    /// (radians, world) — for headings the game imposes: spawn, teleport.
    pub fn align_playspace(&mut self, look: f32) {
        self.playspace_yaw = wrap_pi(look - self.head_yaw());
    }

    /// Bare-hand joints located this frame, left then right. `None` per
    /// hand while the runtime reports inactive tracking.
    pub fn hand_joints(&self) -> [Option<HandJoints>; 2] {
        self.hands.as_ref().map_or([None; 2], |h| *h.joints())
    }

    /// Per-eye render target size the runtime recommends.
    pub fn view_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Width-to-height ratio of what an eye actually sees, in tangent
    /// space — the shape the image must fill, which is the headset's field
    /// of view and not its pixel count. Falls back to the pixel aspect
    /// until the runtime has reported a field of view.
    pub fn fov_aspect(&self) -> f32 {
        self.fov_aspect
            .unwrap_or_else(|| self.width as f32 / self.height as f32)
    }

    /// Opens the runtime, builds the wgpu device on its `MTLDevice`, and
    /// creates one combined stereo swapchain.
    pub fn new(app_name: &str, snap_turn: SnapTurn) -> Result<Self, VrError> {
        let instance = Instance::new(app_name)?;
        let system = instance.system()?;
        let (width, height) = instance.view_size(system)?;

        let raw_device = instance.metal_device(system)?;
        // SAFETY: the runtime returns a live `id<MTLDevice>` that outlives
        // the session; retain takes our own reference.
        let mtl_device: Retained<ProtocolObject<dyn MTLDevice>> =
            unsafe { Retained::retain(raw_device.cast()) }.ok_or(VrError::NoMetalDevice)?;
        let mtl_queue = mtl_device.newCommandQueue().ok_or(VrError::NoMetalDevice)?;

        let (wgpu_instance, device, queue) = build_wgpu(&mtl_queue)?;

        let binding = sys::GraphicsBindingMetalKHR {
            ty: sys::GraphicsBindingMetalKHR::TYPE,
            next: std::ptr::null(),
            command_queue: Retained::as_ptr(&mtl_queue) as *mut c_void,
        };
        let create_info = sys::SessionCreateInfo {
            ty: sys::SessionCreateInfo::TYPE,
            next: std::ptr::from_ref(&binding).cast(),
            create_flags: sys::SessionCreateFlags::EMPTY,
            system_id: system,
        };
        let mut handle = sys::Session::NULL;
        // SAFETY: binding and info outlive the call.
        xr::check("xrCreateSession", unsafe {
            (instance.create_session)(instance.handle, &create_info, &mut handle)
        })?;

        let space_info = sys::ReferenceSpaceCreateInfo {
            ty: sys::ReferenceSpaceCreateInfo::TYPE,
            next: std::ptr::null(),
            reference_space_type: sys::ReferenceSpaceType::LOCAL,
            pose_in_reference_space: identity_pose(),
        };
        let mut space = sys::Space::NULL;
        // SAFETY: info outlives the call.
        xr::check("xrCreateReferenceSpace", unsafe {
            (instance.create_reference_space)(handle, &space_info, &mut space)
        })?;

        let swapchain =
            make_swapchain(&instance, handle, &device, width * EYE_COUNT as u32, height)?;

        info!("vr: session up, {width}x{height} per eye");
        let mut session = Self {
            instance,
            handle,
            space,
            swapchain,
            device,
            queue,
            wgpu_instance,
            width,
            height,
            running: false,
            fov_aspect: None,
            last_views: None,
            actions: None,
            controls: ControllerState::default(),
            hands: None,
            playspace_yaw: 0.0,
            ui_active: false,
        };
        session.hands = match Hands::new(&session.instance, session.handle) {
            Some(Ok(h)) => Some(h),
            Some(Err(e)) => {
                warn!("vr: hand tracking setup failed: {e}");
                None
            }
            None => None,
        };
        // Action sets attach once, and only before the session begins.
        match Actions::new(&session.instance, session.handle, snap_turn) {
            Ok(a) => session.actions = Some(a),
            Err(e) => warn!("vr: controllers unavailable ({e}); keyboard only"),
        }
        session.learn_fov();
        Ok(session)
    }

    /// Runs empty frames until the runtime reports a field of view, so the
    /// caller can size its render buffer before drawing anything. Gives up
    /// quietly after [`FOV_PRIME_DEADLINE`] and leaves the pixel-aspect
    /// fallback in place.
    fn learn_fov(&mut self) {
        let deadline = std::time::Instant::now() + FOV_PRIME_DEADLINE;
        while self.fov_aspect.is_none() && std::time::Instant::now() < deadline {
            match self.poll() {
                Ok(true) => {}
                Ok(false) => {
                    std::thread::sleep(FOV_PRIME_POLL);
                    continue;
                }
                Err(e) => {
                    warn!("vr: priming poll failed: {e}");
                    return;
                }
            }
            // An empty frame is legal and is the only way to reach
            // xrLocateViews, which is what carries the field of view.
            let framed = self.begin_frame().and_then(|frame| self.end_frame(&frame));
            if let Err(e) = framed {
                warn!("vr: priming frame failed: {e}");
                return;
            }
        }
        match self.fov_aspect {
            Some(a) => info!("vr: fov aspect {a:.3}"),
            None => warn!("vr: no field of view reported; using the pixel aspect"),
        }
    }

    /// Drains runtime events, beginning the session when it turns READY and
    /// reporting `false` once it stops.
    pub fn poll(&mut self) -> Result<bool, VrError> {
        // One buffer for the drain: it is 4KB, and this runs every frame.
        let mut buf = sys::EventDataBuffer {
            ty: sys::EventDataBuffer::TYPE,
            next: std::ptr::null(),
            varying: [0; 4000],
        };
        loop {
            buf.ty = sys::EventDataBuffer::TYPE;
            // SAFETY: buffer is spec-sized for any event.
            let result = unsafe { (self.instance.poll_event)(self.instance.handle, &mut buf) };
            if result == sys::Result::EVENT_UNAVAILABLE {
                return Ok(self.running);
            }
            xr::check("xrPollEvent", result)?;
            if buf.ty == sys::EventDataInteractionProfileChanged::TYPE {
                if let Err(error) = self.instance.log_interaction_profiles(self.handle) {
                    warn!("vr: reading interaction profiles: {error}");
                }
                continue;
            }
            if buf.ty != sys::EventDataSessionStateChanged::TYPE {
                continue;
            }
            // SAFETY: the type tag identifies the payload.
            let changed = unsafe {
                std::ptr::from_ref(&buf)
                    .cast::<sys::EventDataSessionStateChanged>()
                    .read()
            };
            match changed.state {
                sys::SessionState::READY => {
                    let begin = sys::SessionBeginInfo {
                        ty: sys::SessionBeginInfo::TYPE,
                        next: std::ptr::null(),
                        primary_view_configuration_type: sys::ViewConfigurationType::PRIMARY_STEREO,
                    };
                    // SAFETY: info outlives the call.
                    xr::check("xrBeginSession", unsafe {
                        (self.instance.begin_session)(self.handle, &begin)
                    })?;
                    self.running = true;
                    info!("vr: session running");
                }
                sys::SessionState::STOPPING => {
                    // The spec's stop handshake: the app must answer
                    // STOPPING with xrEndSession.
                    xr::check("xrEndSession", unsafe {
                        (self.instance.end_session)(self.handle)
                    })?;
                    self.running = false;
                    info!("vr: session stopped");
                }
                sys::SessionState::EXITING => {
                    self.running = false;
                    info!("vr: session exiting");
                }
                _ => {}
            }
        }
    }

    /// Waits for the runtime's frame slot, acquires the stereo image, and
    /// returns the per-eye targets to render into. Drains runtime events
    /// first — a session asked to stop refuses the frame.
    pub fn begin_frame(&mut self) -> Result<FrameContext, VrError> {
        self.controls = ControllerState::default();
        if !self.poll()? {
            return Err(VrError::Stopped);
        }
        let mut state = sys::FrameState {
            ty: sys::FrameState::TYPE,
            next: std::ptr::null_mut(),
            predicted_display_time: sys::Time::from_nanos(0),
            predicted_display_period: sys::Duration::from_nanos(0),
            should_render: sys::FALSE,
        };
        let wait = sys::FrameWaitInfo {
            ty: sys::FrameWaitInfo::TYPE,
            next: std::ptr::null(),
        };
        // SAFETY: out struct is initialised; the runtime paces this call.
        xr::check("xrWaitFrame", unsafe {
            (self.instance.wait_frame)(self.handle, &wait, &mut state)
        })?;
        let begin = sys::FrameBeginInfo {
            ty: sys::FrameBeginInfo::TYPE,
            next: std::ptr::null(),
        };
        // SAFETY: session is live.
        xr::check("xrBeginFrame", unsafe {
            (self.instance.begin_frame)(self.handle, &begin)
        })?;

        // One sync per frame, before anything reads the controllers.
        if let Some(actions) = self.actions.as_mut() {
            match actions.sync(&self.instance, self.handle, self.ui_active) {
                Ok(state) => self.controls = state,
                Err(e) => {
                    warn!("vr: action sync failed: {e}");
                    self.controls = ControllerState::default();
                }
            }
            match actions.poses(
                &self.instance,
                self.handle,
                self.space,
                state.predicted_display_time,
            ) {
                Ok([aim, aim_off, grip]) => {
                    self.controls.aim = aim;
                    self.controls.aim_off = aim_off;
                    self.controls.grip = grip;
                }
                Err(error) => warn!("vr: action poses failed: {error}"),
            }
        }

        if let Some(hands) = self.hands.as_mut() {
            hands.locate(&self.instance, self.space, state.predicted_display_time);
        }

        let (views, flags) = self.locate_views(state.predicted_display_time)?;
        let valid = flags
            .contains(sys::ViewStateFlags::ORIENTATION_VALID | sys::ViewStateFlags::POSITION_VALID);
        // Invalid poses (tracking not settled, or lost) carry undefined
        // fields; hold the last valid views rather than render from them.
        let views = if valid {
            self.playspace_yaw = wrap_pi(self.playspace_yaw + self.controls.turn_radians);
            self.last_views = Some(views);
            views
        } else {
            // A held view is only a rendering fallback. It must not turn lost
            // head tracking into a trigger release or a movement command.
            self.controls = ControllerState::default();
            self.last_views.unwrap_or(views)
        };
        let mut index = 0;
        // SAFETY: the swapchain is live and the output is written on success.
        xr::check("xrAcquireSwapchainImage", unsafe {
            (self.instance.acquire_swapchain_image)(
                self.swapchain.handle,
                std::ptr::null(),
                &mut index,
            )
        })?;
        self.swapchain.acquired = Some(index);
        let wait = sys::SwapchainImageWaitInfo {
            ty: sys::SwapchainImageWaitInfo::TYPE,
            next: std::ptr::null(),
            timeout: sys::Duration::INFINITE,
        };
        // SAFETY: the wait info outlives the call.
        if let Err(error) = xr::check("xrWaitSwapchainImage", unsafe {
            (self.instance.wait_swapchain_image)(self.swapchain.handle, &wait)
        }) {
            self.release_acquired();
            return Err(error.into());
        }

        // The field of view is fixed by the optics, so the first frame that
        // reports one settles the render aspect for the session.
        if self.fov_aspect.is_none()
            && let Some(view) = views.first()
        {
            let width = view.fov.angle_right.tan() - view.fov.angle_left.tan();
            let height = view.fov.angle_up.tan() - view.fov.angle_down.tan();
            if width > 0.0 && height > 0.0 {
                self.fov_aspect = Some(width / height);
            }
        }

        Ok(FrameContext {
            display_time_ns: state.predicted_display_time.as_nanos(),
            should_render: state.should_render == sys::TRUE,
            views,
        })
    }

    fn release_acquired(&mut self) {
        if self.swapchain.acquired.take().is_some() {
            // SAFETY: the swapchain is live and its image was acquired.
            let released = xr::check("xrReleaseSwapchainImage", unsafe {
                (self.instance.release_swapchain_image)(self.swapchain.handle, std::ptr::null())
            });
            if let Err(error) = released {
                warn!("vr: {error}");
            }
        }
    }

    /// The render target for one eye during a frame opened by
    /// [`Self::begin_frame`].
    pub fn eye_target(&self, eye: usize, frame: &FrameContext) -> Option<EyeTarget<'_>> {
        if eye >= EYE_COUNT {
            return None;
        }
        let index = self.swapchain.acquired? as usize;
        let view = frame.views.get(eye)?;
        let p = view.pose.position;
        let q = view.pose.orientation;
        Some(EyeTarget {
            colour: self.swapchain.views.get(index)?,
            rect: [eye as u32 * self.width, 0, self.width, self.height],
            camera: EyeCamera {
                playspace_yaw: self.playspace_yaw,
                position: [p.x, p.y, p.z],
                orientation: [q.x, q.y, q.z, q.w],
                fov: [
                    view.fov.angle_left,
                    view.fov.angle_right,
                    view.fov.angle_up,
                    view.fov.angle_down,
                ],
            },
        })
    }

    /// Releases the image and submits the projection layer. The submit
    /// still runs when a release fails — xrBeginFrame must always be
    /// answered by xrEndFrame or the frame loop jams.
    pub fn end_frame(&mut self, frame: &FrameContext) -> Result<(), VrError> {
        self.release_acquired();

        let projection_views: [sys::CompositionLayerProjectionView; EYE_COUNT] =
            std::array::from_fn(|i| sys::CompositionLayerProjectionView {
                ty: sys::CompositionLayerProjectionView::TYPE,
                next: std::ptr::null(),
                pose: frame.views[i].pose,
                fov: frame.views[i].fov,
                sub_image: sys::SwapchainSubImage {
                    swapchain: self.swapchain.handle,
                    image_rect: sys::Rect2Di {
                        offset: sys::Offset2Di {
                            x: i as i32 * self.width as i32,
                            y: 0,
                        },
                        extent: sys::Extent2Di {
                            width: self.width as i32,
                            height: self.height as i32,
                        },
                    },
                    image_array_index: 0,
                },
            });
        let layer = sys::CompositionLayerProjection {
            ty: sys::CompositionLayerProjection::TYPE,
            next: std::ptr::null(),
            layer_flags: sys::CompositionLayerFlags::EMPTY,
            space: self.space,
            view_count: EYE_COUNT as u32,
            views: projection_views.as_ptr(),
        };
        let layers: [*const sys::CompositionLayerBaseHeader; 1] =
            [std::ptr::from_ref(&layer).cast()];
        let end = sys::FrameEndInfo {
            ty: sys::FrameEndInfo::TYPE,
            next: std::ptr::null(),
            display_time: sys::Time::from_nanos(frame.display_time_ns),
            environment_blend_mode: sys::EnvironmentBlendMode::OPAQUE,
            layer_count: 1,
            layers: layers.as_ptr(),
        };
        // SAFETY: every pointed-to struct outlives the call.
        xr::check("xrEndFrame", unsafe {
            (self.instance.end_frame)(self.handle, &end)
        })?;
        Ok(())
    }

    fn locate_views(
        &self,
        at: sys::Time,
    ) -> Result<([sys::View; EYE_COUNT], sys::ViewStateFlags), VrError> {
        let locate = sys::ViewLocateInfo {
            ty: sys::ViewLocateInfo::TYPE,
            next: std::ptr::null(),
            view_configuration_type: sys::ViewConfigurationType::PRIMARY_STEREO,
            display_time: at,
            space: self.space,
        };
        let mut state = sys::ViewState {
            ty: sys::ViewState::TYPE,
            next: std::ptr::null_mut(),
            view_state_flags: sys::ViewStateFlags::EMPTY,
        };
        let mut views = [sys::View {
            ty: sys::View::TYPE,
            next: std::ptr::null_mut(),
            pose: identity_pose(),
            fov: sys::Fovf {
                angle_left: 0.0,
                angle_right: 0.0,
                angle_up: 0.0,
                angle_down: 0.0,
            },
        }; EYE_COUNT];
        let mut count = 0;
        // SAFETY: `views` has EYE_COUNT entries; all inputs outlive the call.
        xr::check("xrLocateViews", unsafe {
            (self.instance.locate_views)(
                self.handle,
                &locate,
                &mut state,
                EYE_COUNT as u32,
                &mut count,
                views.as_mut_ptr(),
            )
        })?;
        Ok((views, state.view_state_flags))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Runtime handles only; the wgpu wrappers dropped after this body
        // hold their own Metal references, so destruction order is safe.
        // Innermost first, as the spec requires.
        // SAFETY: handles are live; nothing uses them after this.
        unsafe {
            if let Some(hands) = &self.hands {
                hands.destroy(&self.instance);
            }
            if let Some(actions) = &self.actions {
                actions.destroy(&self.instance);
            }
            destroyed(
                "xrDestroySwapchain",
                (self.instance.destroy_swapchain)(self.swapchain.handle),
            );
            destroyed("xrDestroySpace", (self.instance.destroy_space)(self.space));
            destroyed(
                "xrDestroySession",
                (self.instance.destroy_session)(self.handle),
            );
        }
    }
}

/// Logs a teardown call that failed. Nothing can be done about it — the
/// process is on its way out — but a silent leak is worth knowing about.
fn destroyed(what: &str, result: sys::Result) {
    if result != sys::Result::SUCCESS {
        warn!("vr: {what} failed: {}", result.into_raw());
    }
}

/// What the runtime says about the frame just opened.
pub struct FrameContext {
    pub display_time_ns: i64,
    /// False when the runtime wants the frame skipped (headset off, etc.).
    pub should_render: bool,
    views: [sys::View; EYE_COUNT],
}

fn identity_pose() -> sys::Posef {
    sys::Posef {
        orientation: sys::Quaternionf {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
        position: sys::Vector3f {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    }
}

/// Creates one eye's swapchain and wraps every runtime image as a wgpu view.
fn make_swapchain(
    instance: &Instance,
    session: sys::Session,
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Result<Swapchain, VrError> {
    let create_info = sys::SwapchainCreateInfo {
        ty: sys::SwapchainCreateInfo::TYPE,
        next: std::ptr::null(),
        create_flags: sys::SwapchainCreateFlags::EMPTY,
        usage_flags: sys::SwapchainUsageFlags::COLOR_ATTACHMENT | sys::SwapchainUsageFlags::SAMPLED,
        format: MTL_BGRA8_UNORM_SRGB,
        sample_count: 1,
        width,
        height,
        face_count: 1,
        array_size: 1,
        mip_count: 1,
    };
    let mut handle = sys::Swapchain::NULL;
    // SAFETY: info outlives the call.
    xr::check("xrCreateSwapchain", unsafe {
        (instance.create_swapchain)(session, &create_info, &mut handle)
    })?;

    let mut count = 0;
    // SAFETY: two-call idiom; first pass writes only the count.
    xr::check("xrEnumerateSwapchainImages", unsafe {
        (instance.enumerate_swapchain_images)(handle, 0, &mut count, std::ptr::null_mut())
    })?;
    let mut images = vec![
        sys::SwapchainImageMetalKHR {
            ty: sys::SwapchainImageMetalKHR::TYPE,
            next: std::ptr::null_mut(),
            texture: std::ptr::null_mut(),
        };
        count as usize
    ];
    // SAFETY: `images` has `count` entries of the Metal image type.
    xr::check("xrEnumerateSwapchainImages", unsafe {
        (instance.enumerate_swapchain_images)(handle, count, &mut count, images.as_mut_ptr().cast())
    })?;

    let views = images
        .iter()
        .map(|image| wrap_texture(device, image.texture, width, height))
        .collect::<Result<Vec<_>, _>>();
    let views = match views {
        Ok(views) => views,
        Err(error) => {
            // SAFETY: this call created the handle and no image is acquired.
            destroyed("xrDestroySwapchain", unsafe {
                (instance.destroy_swapchain)(handle)
            });
            return Err(error);
        }
    };

    Ok(Swapchain {
        handle,
        views,
        acquired: None,
    })
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{EYE_COUNT, Session, SnapTurn, VrError};

    #[test]
    #[ignore = "requires an installed Metal OpenXR runtime and a Streaming headset"]
    fn installed_runtime_renders_stereo_frames() -> Result<(), VrError> {
        let mut session = Session::new("room4doom-session-test", SnapTurn::Deg30)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut rendered = 0;
        while rendered < 3 && Instant::now() < deadline {
            if !session.poll()? {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            let frame = session.begin_frame()?;
            if frame.should_render {
                let mut encoder =
                    session
                        .device()
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("OpenXR smoke frame"),
                        });
                for eye in 0..EYE_COUNT {
                    let target = session
                        .eye_target(eye, &frame)
                        .expect("a renderable frame supplies both acquired eye targets");
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target.colour,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                session.queue().submit([encoder.finish()]);
                rendered += 1;
            }
            session.end_frame(&frame)?;
        }
        assert_eq!(
            rendered, 3,
            "runtime did not admit stereo rendering within five seconds"
        );
        Ok(())
    }
}
