//! The raw OpenXR calls, kept behind a small safe surface.
//!
//! `openxr-sys` is used directly rather than the `openxr` wrapper crate: the
//! wrapper's Metal path is unproven, while the generated Metal structs
//! (`GraphicsBindingMetalKHR`, `SwapchainImageMetalKHR`,
//! `GraphicsRequirementsMetalKHR`) are present and correct.

use std::ffi::{CStr, CString, c_char, c_void};
use std::fmt;
use std::sync::OnceLock;

use openxr_sys as sys;
use openxr_sys::Handle as _;

/// The extension the Metal graphics binding lives behind.
pub const METAL_EXTENSION: &str = "XR_KHR_metal_enable";
/// Bare-hand joints; optional, so its absence is not an error.
pub const HAND_TRACKING_EXTENSION: &str = "XR_EXT_hand_tracking";
/// Runtime-recognised hand gestures as ordinary actions; also optional. It must
/// be enabled here or its interaction profile cannot be bound.
pub const HAND_INTERACTION_EXTENSION: &str = "XR_EXT_hand_interaction";

/// The `XR_EXT_hand_tracking` entry points, resolved together.
pub struct HandTracking {
    pub create: sys::pfn::CreateHandTrackerEXT,
    pub destroy: sys::pfn::DestroyHandTrackerEXT,
    pub locate: sys::pfn::LocateHandJointsEXT,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrError {
    /// A runtime call returned a failure code.
    Call { what: &'static str, result: i32 },
    /// The runtime does not advertise `XR_KHR_metal_enable`.
    NoMetalExtension,
    /// `libopenxr_loader` could not be opened.
    NoLoader,
    /// A function the Metal path needs is not exported.
    MissingFunction(&'static str),
}

impl fmt::Display for XrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call {
                what,
                result,
            } => write!(f, "{what} failed: {result}"),
            Self::NoMetalExtension => {
                write!(f, "runtime does not support {METAL_EXTENSION}")
            }
            Self::NoLoader => write!(
                f,
                "libopenxr_loader not found (set {LOADER_PATH_VAR} or DYLD_FALLBACK_LIBRARY_PATH)"
            ),
            Self::MissingFunction(name) => write!(f, "runtime does not export {name}"),
        }
    }
}

/// Turns a runtime result code into an error naming the call that failed.
pub fn check(what: &'static str, result: sys::Result) -> Result<(), XrError> {
    if result == sys::Result::SUCCESS {
        Ok(())
    } else {
        Err(XrError::Call {
            what,
            result: result.into_raw(),
        })
    }
}

/// Interns a semantic path string with the runtime.
///
/// # Safety
/// `instance` must be live.
pub unsafe fn intern_path(instance: &Instance, path: &str) -> Result<sys::Path, XrError> {
    let cpath = CString::new(path).expect("paths are ASCII literals");
    let mut out = sys::Path::NULL;
    // SAFETY: caller contract; the string outlives the call.
    check("xrStringToPath", unsafe {
        (instance.string_to_path)(instance.handle, cpath.as_ptr(), &mut out)
    })?;
    Ok(out)
}

/// Copies a Rust string into a fixed OpenXR name buffer.
pub fn write_name(dst: &mut [c_char], s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len() - 1);
    for (d, b) in dst.iter_mut().zip(bytes.iter().take(n)) {
        *d = *b as c_char;
    }
    dst[n] = 0;
}

/// Extension names the loader's runtime advertises.
pub fn supported_extensions() -> Result<Vec<String>, XrError> {
    // SAFETY: the name matches this signature in the OpenXR ABI.
    let enumerate_extensions: sys::pfn::EnumerateInstanceExtensionProperties =
        unsafe { global_proc("xrEnumerateInstanceExtensionProperties")? };
    let mut count = 0;
    // SAFETY: two-call idiom; the first pass writes only the count.
    check("xrEnumerateInstanceExtensionProperties", unsafe {
        enumerate_extensions(std::ptr::null(), 0, &mut count, std::ptr::null_mut())
    })?;
    let mut props = vec![
        sys::ExtensionProperties {
            ty: sys::ExtensionProperties::TYPE,
            next: std::ptr::null_mut(),
            extension_name: [0; sys::MAX_EXTENSION_NAME_SIZE],
            extension_version: 0,
        };
        count as usize
    ];
    // SAFETY: `props` has `count` entries, as the first call reported.
    check("xrEnumerateInstanceExtensionProperties", unsafe {
        enumerate_extensions(std::ptr::null(), count, &mut count, props.as_mut_ptr())
    })?;
    Ok(props
        .iter()
        .map(|p| {
            // SAFETY: the runtime NUL-terminates every name.
            let name = unsafe { CStr::from_ptr(p.extension_name.as_ptr()) };
            name.to_string_lossy().into_owned()
        })
        .collect())
}

/// The loader, opened at runtime.
///
/// Linking it would pin the build to a loader present at compile time;
/// OpenXR's own contract is `dlopen` + `xrGetInstanceProcAddr`, which also
/// lets `XR_RUNTIME_JSON` select a runtime per run. Override the library
/// path with `OPENXR_LOADER` when it is not on the dynamic search path.
struct Loader {
    /// Kept for the process lifetime; every resolved pointer borrows it.
    _handle: *mut c_void,
    get_instance_proc_addr: sys::pfn::GetInstanceProcAddr,
}

// SAFETY: the loader is opened once and only read afterwards.
unsafe impl Send for Loader {}
unsafe impl Sync for Loader {}

const LOADER_PATH_VAR: &str = "OPENXR_LOADER";
const LOADER_NAMES: [&str; 2] = ["libopenxr_loader.dylib", "libopenxr_loader.1.dylib"];

static LOADER: OnceLock<Option<Loader>> = OnceLock::new();

fn loader() -> Result<&'static Loader, XrError> {
    LOADER
        .get_or_init(|| {
            let mut candidates: Vec<String> = std::env::var(LOADER_PATH_VAR).into_iter().collect();
            candidates.extend(LOADER_NAMES.iter().map(|s| (*s).to_owned()));
            candidates.iter().find_map(|name| {
                let cname = CString::new(name.as_str()).ok()?;
                // SAFETY: plain dlopen of a caller-named library.
                let handle = unsafe { libc::dlopen(cname.as_ptr(), libc::RTLD_NOW) };
                if handle.is_null() {
                    return None;
                }
                let sym = CString::new("xrGetInstanceProcAddr").expect("literal is ASCII");
                // SAFETY: the loader exports this symbol by ABI contract.
                let addr = unsafe { libc::dlsym(handle, sym.as_ptr()) };
                if addr.is_null() {
                    return None;
                }
                // SAFETY: the symbol has the ABI its name declares.
                let get_instance_proc_addr = unsafe {
                    std::mem::transmute::<*mut c_void, sys::pfn::GetInstanceProcAddr>(addr)
                };
                Some(Loader {
                    _handle: handle,
                    get_instance_proc_addr,
                })
            })
        })
        .as_ref()
        .ok_or(XrError::NoLoader)
}

/// Resolves a global (instance-less) entry point through the loader.
///
/// # Safety
/// `F` must match the signature the named function actually has.
unsafe fn global_proc<F>(name: &'static str) -> Result<F, XrError> {
    // SAFETY: caller contract; NULL instance is legal for global functions.
    unsafe { proc_addr_via(loader()?.get_instance_proc_addr, sys::Instance::NULL, name) }
}

/// Resolves one entry point through `gipa`, failing loudly on a null.
///
/// # Safety
/// `F` must match the named function's signature; `instance` must be live
/// or NULL for global functions.
unsafe fn proc_addr_via<F>(
    gipa: sys::pfn::GetInstanceProcAddr,
    instance: sys::Instance,
    name: &'static str,
) -> Result<F, XrError> {
    let cname = CString::new(name).expect("entry-point names are ASCII literals");
    let mut f: Option<sys::pfn::VoidFunction> = None;
    // SAFETY: caller contract; out-param written on success.
    let result = unsafe { gipa(instance, cname.as_ptr(), &mut f) };
    if result != sys::Result::SUCCESS {
        return Err(XrError::MissingFunction(name));
    }
    let f = f.ok_or(XrError::MissingFunction(name))?;
    // SAFETY: the loader returns a pointer whose signature matches the name
    // it was looked up by.
    Ok(unsafe { std::mem::transmute_copy::<sys::pfn::VoidFunction, F>(&f) })
}

/// A live instance plus the entry points the Metal path needs.
pub struct Instance {
    pub handle: sys::Instance,
    pub get_metal_requirements: sys::pfn::GetMetalGraphicsRequirementsKHR,
    pub get_system: sys::pfn::GetSystem,
    pub enumerate_view_configuration_views: sys::pfn::EnumerateViewConfigurationViews,
    pub create_session: sys::pfn::CreateSession,
    pub begin_session: sys::pfn::BeginSession,
    pub create_reference_space: sys::pfn::CreateReferenceSpace,
    pub create_swapchain: sys::pfn::CreateSwapchain,
    pub enumerate_swapchain_images: sys::pfn::EnumerateSwapchainImages,
    pub acquire_swapchain_image: sys::pfn::AcquireSwapchainImage,
    pub wait_swapchain_image: sys::pfn::WaitSwapchainImage,
    pub release_swapchain_image: sys::pfn::ReleaseSwapchainImage,
    pub wait_frame: sys::pfn::WaitFrame,
    pub begin_frame: sys::pfn::BeginFrame,
    pub end_frame: sys::pfn::EndFrame,
    pub locate_views: sys::pfn::LocateViews,
    pub poll_event: sys::pfn::PollEvent,
    /// `XR_EXT_hand_tracking`; `None` when the runtime lacks it.
    pub hand_tracking: Option<HandTracking>,
    pub string_to_path: sys::pfn::StringToPath,
    pub create_action_set: sys::pfn::CreateActionSet,
    pub create_action: sys::pfn::CreateAction,
    pub suggest_interaction_profile_bindings: sys::pfn::SuggestInteractionProfileBindings,
    pub attach_session_action_sets: sys::pfn::AttachSessionActionSets,
    pub sync_actions: sys::pfn::SyncActions,
    pub get_action_state_boolean: sys::pfn::GetActionStateBoolean,
    pub get_action_state_vector2f: sys::pfn::GetActionStateVector2f,
    pub create_action_space: sys::pfn::CreateActionSpace,
    pub locate_space: sys::pfn::LocateSpace,
    pub destroy_action_set: sys::pfn::DestroyActionSet,
    pub end_session: sys::pfn::EndSession,
    pub destroy_session: sys::pfn::DestroySession,
    pub destroy_space: sys::pfn::DestroySpace,
    pub destroy_swapchain: sys::pfn::DestroySwapchain,
    pub destroy_instance: sys::pfn::DestroyInstance,
}

/// Looks up one instance entry point.
///
/// # Safety
/// `instance` must be live; the caller must name a function whose signature
/// matches `F`.
unsafe fn proc_addr<F>(instance: sys::Instance, name: &'static str) -> Result<F, XrError> {
    // SAFETY: caller contract.
    unsafe { proc_addr_via(loader()?.get_instance_proc_addr, instance, name) }
}

impl Instance {
    /// Creates an instance with the Metal extension enabled.
    pub fn new(app_name: &str) -> Result<Self, XrError> {
        let available = supported_extensions()?;
        if !available.iter().any(|e| e == METAL_EXTENSION) {
            return Err(XrError::NoMetalExtension);
        }
        // The hand extensions are a bonus: enable each when offered, carry on
        // if not. Without them a runtime just never reports bare hands.
        let offered = |ext: &str| available.iter().any(|e| e == ext);
        let hands_offered = offered(HAND_TRACKING_EXTENSION);
        let names: Vec<CString> = std::iter::once(METAL_EXTENSION)
            .chain(hands_offered.then_some(HAND_TRACKING_EXTENSION))
            .chain(offered(HAND_INTERACTION_EXTENSION).then_some(HAND_INTERACTION_EXTENSION))
            .map(|e| CString::new(e).expect("literal is ASCII"))
            .collect();
        let ext_ptrs: Vec<*const c_char> = names.iter().map(|n| n.as_ptr()).collect();

        let mut app_info = sys::ApplicationInfo {
            application_name: [0; sys::MAX_APPLICATION_NAME_SIZE],
            application_version: 1,
            engine_name: [0; sys::MAX_ENGINE_NAME_SIZE],
            engine_version: 1,
            api_version: sys::CURRENT_API_VERSION,
        };
        write_name(&mut app_info.application_name, app_name);
        write_name(&mut app_info.engine_name, "room4doom");

        let info = sys::InstanceCreateInfo {
            ty: sys::InstanceCreateInfo::TYPE,
            next: std::ptr::null(),
            create_flags: sys::InstanceCreateFlags::EMPTY,
            application_info: app_info,
            enabled_api_layer_count: 0,
            enabled_api_layer_names: std::ptr::null(),
            enabled_extension_count: ext_ptrs.len() as u32,
            enabled_extension_names: ext_ptrs.as_ptr(),
        };
        // SAFETY: the name matches this signature in the OpenXR ABI.
        let create_instance: sys::pfn::CreateInstance = unsafe { global_proc("xrCreateInstance")? };
        let mut handle = sys::Instance::NULL;
        // SAFETY: info is fully initialised and outlives the call.
        check("xrCreateInstance", unsafe {
            create_instance(&info, &mut handle)
        })?;

        // SAFETY: instance is live; each name matches the stored signature.
        unsafe {
            Ok(Self {
                handle,
                get_metal_requirements: proc_addr(handle, "xrGetMetalGraphicsRequirementsKHR")?,
                get_system: proc_addr(handle, "xrGetSystem")?,
                enumerate_view_configuration_views: proc_addr(
                    handle,
                    "xrEnumerateViewConfigurationViews",
                )?,
                create_session: proc_addr(handle, "xrCreateSession")?,
                begin_session: proc_addr(handle, "xrBeginSession")?,
                create_reference_space: proc_addr(handle, "xrCreateReferenceSpace")?,
                create_swapchain: proc_addr(handle, "xrCreateSwapchain")?,
                enumerate_swapchain_images: proc_addr(handle, "xrEnumerateSwapchainImages")?,
                acquire_swapchain_image: proc_addr(handle, "xrAcquireSwapchainImage")?,
                wait_swapchain_image: proc_addr(handle, "xrWaitSwapchainImage")?,
                release_swapchain_image: proc_addr(handle, "xrReleaseSwapchainImage")?,
                wait_frame: proc_addr(handle, "xrWaitFrame")?,
                begin_frame: proc_addr(handle, "xrBeginFrame")?,
                end_frame: proc_addr(handle, "xrEndFrame")?,
                locate_views: proc_addr(handle, "xrLocateViews")?,
                poll_event: proc_addr(handle, "xrPollEvent")?,
                hand_tracking: hands_offered
                    .then(|| {
                        Some(HandTracking {
                            create: proc_addr(handle, "xrCreateHandTrackerEXT").ok()?,
                            destroy: proc_addr(handle, "xrDestroyHandTrackerEXT").ok()?,
                            locate: proc_addr(handle, "xrLocateHandJointsEXT").ok()?,
                        })
                    })
                    .flatten(),
                string_to_path: proc_addr(handle, "xrStringToPath")?,
                create_action_set: proc_addr(handle, "xrCreateActionSet")?,
                create_action: proc_addr(handle, "xrCreateAction")?,
                suggest_interaction_profile_bindings: proc_addr(
                    handle,
                    "xrSuggestInteractionProfileBindings",
                )?,
                attach_session_action_sets: proc_addr(handle, "xrAttachSessionActionSets")?,
                sync_actions: proc_addr(handle, "xrSyncActions")?,
                get_action_state_boolean: proc_addr(handle, "xrGetActionStateBoolean")?,
                get_action_state_vector2f: proc_addr(handle, "xrGetActionStateVector2f")?,
                create_action_space: proc_addr(handle, "xrCreateActionSpace")?,
                locate_space: proc_addr(handle, "xrLocateSpace")?,
                destroy_action_set: proc_addr(handle, "xrDestroyActionSet")?,
                end_session: proc_addr(handle, "xrEndSession")?,
                destroy_session: proc_addr(handle, "xrDestroySession")?,
                destroy_space: proc_addr(handle, "xrDestroySpace")?,
                destroy_swapchain: proc_addr(handle, "xrDestroySwapchain")?,
                destroy_instance: proc_addr(handle, "xrDestroyInstance")?,
            })
        }
    }

    /// The `MTLDevice` the runtime requires the app to render on.
    pub fn metal_device(&self, system: sys::SystemId) -> Result<*mut c_void, XrError> {
        let mut reqs = sys::GraphicsRequirementsMetalKHR {
            ty: sys::GraphicsRequirementsMetalKHR::TYPE,
            next: std::ptr::null_mut(),
            metal_device: std::ptr::null_mut(),
        };
        // SAFETY: instance and system are live; out struct is initialised.
        check("xrGetMetalGraphicsRequirementsKHR", unsafe {
            (self.get_metal_requirements)(self.handle, system, &mut reqs)
        })?;
        Ok(reqs.metal_device)
    }

    pub fn system(&self) -> Result<sys::SystemId, XrError> {
        let info = sys::SystemGetInfo {
            ty: sys::SystemGetInfo::TYPE,
            next: std::ptr::null(),
            form_factor: sys::FormFactor::HEAD_MOUNTED_DISPLAY,
        };
        let mut id = sys::SystemId::NULL;
        // SAFETY: info outlives the call; out-param written on success.
        check("xrGetSystem", unsafe {
            (self.get_system)(self.handle, &info, &mut id)
        })?;
        Ok(id)
    }

    /// Recommended per-eye render target size.
    pub fn view_size(&self, system: sys::SystemId) -> Result<(u32, u32), XrError> {
        let mut count = 0;
        // SAFETY: two-call idiom; first pass writes only the count.
        check("xrEnumerateViewConfigurationViews", unsafe {
            (self.enumerate_view_configuration_views)(
                self.handle,
                system,
                sys::ViewConfigurationType::PRIMARY_STEREO,
                0,
                &mut count,
                std::ptr::null_mut(),
            )
        })?;
        let mut views = vec![
            sys::ViewConfigurationView {
                ty: sys::ViewConfigurationView::TYPE,
                next: std::ptr::null_mut(),
                recommended_image_rect_width: 0,
                max_image_rect_width: 0,
                recommended_image_rect_height: 0,
                max_image_rect_height: 0,
                recommended_swapchain_sample_count: 0,
                max_swapchain_sample_count: 0,
            };
            count as usize
        ];
        // SAFETY: `views` has `count` entries.
        check("xrEnumerateViewConfigurationViews", unsafe {
            (self.enumerate_view_configuration_views)(
                self.handle,
                system,
                sys::ViewConfigurationType::PRIMARY_STEREO,
                count,
                &mut count,
                views.as_mut_ptr(),
            )
        })?;
        let first = views.first().ok_or(XrError::Call {
            what: "xrEnumerateViewConfigurationViews",
            result: 0,
        })?;
        Ok((
            first.recommended_image_rect_width,
            first.recommended_image_rect_height,
        ))
    }
}
