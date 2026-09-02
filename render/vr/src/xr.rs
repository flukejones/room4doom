//! The raw OpenXR calls, kept behind a small safe surface.
//!
//! Loads the standard OpenXR loader and resolves calls through
//! `xrGetInstanceProcAddr`; `XR_RUNTIME_JSON` selects the runtime.

use std::env::var_os;
use std::ffi::{CStr, CString, c_char, c_void};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use libloading::Library;
use libloading::os::unix::{Library as UnixLibrary, RTLD_LOCAL, RTLD_NOW};
use log::{info, warn};
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

#[derive(Debug, Clone)]
pub enum XrError {
    /// A runtime call returned a failure code.
    Call { what: &'static str, result: i32 },
    /// The runtime does not advertise `XR_KHR_metal_enable`.
    NoMetalExtension,
    /// Each attempted loader path and its native library/symbol error.
    NoLoader(Vec<(PathBuf, Arc<libloading::Error>)>),
    /// A function the Metal path needs is not exported.
    MissingFunction(&'static str),
}

impl fmt::Display for XrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call {
                what,
                result,
            } => write!(
                f,
                "{what} failed: {:?} ({result})",
                sys::Result::from_raw(*result)
            ),
            Self::NoMetalExtension => {
                write!(f, "runtime does not support {METAL_EXTENSION}")
            }
            Self::NoLoader(attempts) => {
                write!(f, "could not load OpenXR (override with {LOADER_PATH_VAR})")?;
                for (path, error) in attempts {
                    write!(f, "; {}: {error}", path.display())?;
                }
                Ok(())
            }
            Self::MissingFunction(name) => write!(f, "runtime does not export {name}"),
        }
    }
}

/// Turns a runtime result code into an error naming the call that failed.
pub fn check(what: &'static str, result: sys::Result) -> Result<(), XrError> {
    if result.into_raw() >= 0 {
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

/// Keeps the loader alive for every resolved function pointer.
struct Loader {
    _library: Library,
    get_instance_proc_addr: sys::pfn::GetInstanceProcAddr,
}

const LOADER_PATH_VAR: &str = "OPENXR_LOADER";
const LOADER_NAMES: [&str; 2] = ["libopenxr_loader.dylib", "libopenxr_loader.1.dylib"];

static LOADER: OnceLock<Result<Loader, XrError>> = OnceLock::new();

fn open_loader(path: &Path) -> Result<Loader, libloading::Error> {
    // SAFETY: the selected library must implement the OpenXR loader ABI.
    let library: Library = unsafe { UnixLibrary::open(Some(path), RTLD_NOW | RTLD_LOCAL) }?.into();
    // SAFETY: this exported OpenXR symbol has the declared C ABI signature.
    let get_instance_proc_addr =
        unsafe { *library.get::<sys::pfn::GetInstanceProcAddr>(b"xrGetInstanceProcAddr\0")? };
    Ok(Loader {
        _library: library,
        get_instance_proc_addr,
    })
}

fn loader() -> Result<&'static Loader, XrError> {
    LOADER
        .get_or_init(|| {
            let candidates = if let Some(path) = var_os(LOADER_PATH_VAR) {
                vec![PathBuf::from(path)]
            } else {
                let mut paths: Vec<PathBuf> = LOADER_NAMES.iter().map(PathBuf::from).collect();
                if let Some(home) = var_os("HOME") {
                    paths.push(PathBuf::from(home).join(
                        "Library/Application Support/Headgasket VR/OpenXR/libopenxr_loader.dylib",
                    ));
                }
                paths
            };
            let mut failures = Vec::new();
            for path in candidates {
                match open_loader(&path) {
                    Ok(loader) => {
                        info!("vr: OpenXR loader {}", path.display());
                        return Ok(loader);
                    }
                    Err(error) => failures.push((path, Arc::new(error))),
                }
            }
            Err(XrError::NoLoader(failures))
        })
        .as_ref()
        .map_err(Clone::clone)
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
    check(name, result)?;
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
    /// Whether `XR_EXT_hand_interaction` was enabled; required before suggesting its profile.
    pub hand_interaction: bool,
    pub string_to_path: sys::pfn::StringToPath,
    pub create_action_set: sys::pfn::CreateActionSet,
    pub create_action: sys::pfn::CreateAction,
    pub suggest_interaction_profile_bindings: sys::pfn::SuggestInteractionProfileBindings,
    pub attach_session_action_sets: sys::pfn::AttachSessionActionSets,
    pub sync_actions: sys::pfn::SyncActions,
    pub get_action_state_boolean: sys::pfn::GetActionStateBoolean,
    pub get_action_state_vector2f: sys::pfn::GetActionStateVector2f,
    pub create_action_space: sys::pfn::CreateActionSpace,
    /// Core OpenXR 1.1 batch location; all input spaces must belong to the supplied session.
    pub locate_spaces: sys::pfn::LocateSpaces,
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
    /// Logs the selected hand profiles after an interaction-profile change event.
    /// The session must belong to this instance; queries retain concrete runtime errors.
    pub fn log_interaction_profiles(&self, session: sys::Session) -> Result<(), XrError> {
        // SAFETY: the names and signatures identify core calls on this live instance.
        let (get_profile, path_to_string) = unsafe {
            (
                proc_addr::<sys::pfn::GetCurrentInteractionProfile>(
                    self.handle,
                    "xrGetCurrentInteractionProfile",
                )?,
                proc_addr::<sys::pfn::PathToString>(self.handle, "xrPathToString")?,
            )
        };
        for user in ["/user/hand/left", "/user/hand/right"] {
            // SAFETY: this instance remains live throughout the query.
            let path = unsafe { intern_path(self, user)? };
            let mut state = sys::InteractionProfileState {
                ty: sys::InteractionProfileState::TYPE,
                next: std::ptr::null_mut(),
                interaction_profile: sys::Path::NULL,
            };
            // SAFETY: the session, path and output belong to this live instance.
            check("xrGetCurrentInteractionProfile", unsafe {
                get_profile(session, path, &mut state)
            })?;
            if state.interaction_profile == sys::Path::NULL {
                info!("vr: {user} profile unavailable");
                continue;
            }
            let mut name = [0; sys::MAX_PATH_LENGTH];
            let mut length = 0;
            // SAFETY: the buffer has the capacity supplied and the profile is a runtime path.
            check("xrPathToString", unsafe {
                path_to_string(
                    self.handle,
                    state.interaction_profile,
                    name.len() as u32,
                    &mut length,
                    name.as_mut_ptr(),
                )
            })?;
            // SAFETY: successful xrPathToString writes a NUL-terminated string.
            let name = unsafe { CStr::from_ptr(name.as_ptr()) };
            info!("vr: {user} profile {}", name.to_string_lossy());
        }
        Ok(())
    }

    /// Creates an instance with the Metal extension enabled.
    pub fn new(app_name: &str) -> Result<Self, XrError> {
        let available = supported_extensions()?;
        if !available.iter().any(|e| e == METAL_EXTENSION) {
            return Err(XrError::NoMetalExtension);
        }
        let offered = |ext: &str| available.iter().any(|e| e == ext);
        let hands_offered = offered(HAND_TRACKING_EXTENSION);
        let hand_interaction = offered(HAND_INTERACTION_EXTENSION);
        let names: Vec<CString> = std::iter::once(METAL_EXTENSION)
            .chain(hands_offered.then_some(HAND_TRACKING_EXTENSION))
            .chain(hand_interaction.then_some(HAND_INTERACTION_EXTENSION))
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

        // SAFETY: the live instance supports this core entry point.
        let destroy_instance =
            unsafe { proc_addr::<sys::pfn::DestroyInstance>(handle, "xrDestroyInstance")? };
        let resolved = (|| {
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
                    hand_tracking: if hands_offered {
                        Some(HandTracking {
                            create: proc_addr(handle, "xrCreateHandTrackerEXT")?,
                            destroy: proc_addr(handle, "xrDestroyHandTrackerEXT")?,
                            locate: proc_addr(handle, "xrLocateHandJointsEXT")?,
                        })
                    } else {
                        None
                    },
                    hand_interaction,
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
                    locate_spaces: proc_addr(handle, "xrLocateSpaces")?,
                    destroy_action_set: proc_addr(handle, "xrDestroyActionSet")?,
                    end_session: proc_addr(handle, "xrEndSession")?,
                    destroy_session: proc_addr(handle, "xrDestroySession")?,
                    destroy_space: proc_addr(handle, "xrDestroySpace")?,
                    destroy_swapchain: proc_addr(handle, "xrDestroySwapchain")?,
                    destroy_instance,
                })
            }
        })();
        if resolved.is_err() {
            // SAFETY: failed function resolution has not transferred this instance to an owner.
            let result = unsafe { destroy_instance(handle) };
            if result != sys::Result::SUCCESS {
                warn!("vr: xrDestroyInstance after failed initialization: {result:?}");
            }
        }
        resolved
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

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: the owning session destroys its child handles before dropping the instance.
        let result = unsafe { (self.destroy_instance)(self.handle) };
        if result != sys::Result::SUCCESS {
            warn!("vr: xrDestroyInstance failed: {result:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an installed Metal OpenXR runtime selected by XR_RUNTIME_JSON"]
    fn installed_runtime_resolves_calls_and_releases_instances() -> Result<(), XrError> {
        for _ in 0..2 {
            let instance = Instance::new("room4doom-loader-test")?;
            // SAFETY: the instance is live; unsupported names must return no function pointer.
            let result = unsafe {
                proc_addr::<sys::pfn::VoidFunction>(instance.handle, "xrRoom4DoomUnsupported")
            };
            assert!(matches!(result, Err(XrError::Call { result, .. })
                if result == sys::Result::ERROR_FUNCTION_UNSUPPORTED.into_raw()));
        }
        Ok(())
    }
}
