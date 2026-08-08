# vr

OpenXR stereo backend: session lifecycle, swapchains, frame pacing, controller
and hand input. macOS-only (`XR_KHR_metal_enable`).

Engine-agnostic — nothing here knows about Doom. The game consumes plain data:
per-eye cameras and texture views, controller state, hand joints.

## The chain

```
room4doom ──dlopen──▶ libopenxr_loader ──▶ runtime (headgasket, in-process) ──network──▶ ALVR client (headset)
```

- The loader is dlopened by name; `OPENXR_LOADER=/path/to/libopenxr_loader.dylib`
  overrides when it is off the dynamic search path.
- The loader selects the runtime via `XR_RUNTIME_JSON` (or the installed
  `active_runtime.json`).
- The game sees only OpenXR. Anything the ALVR protocol carries reaches it only
  if the runtime surfaces it as an OpenXR concept: poses as spaces, buttons as
  actions, hand skeletons as `XR_EXT_hand_tracking` joints.

## Rendering

- The runtime names the `MTLDevice` the app must render on; `device.rs` wraps
  it into a wgpu device through `wgpu-hal` (unstable surface — wgpu is pinned).
- `Session` owns the per-eye `Bgra8Unorm` swapchains. Each frame: `xrWaitFrame`
  paces, both eyes are located, the renderer records into the acquired texture
  views, `xrEndFrame` hands them to the compositor.
- The renderer is unmodified: it draws into the texture views exactly as into a
  window's scene target.

## Controllers

- Two action sets: `gameplay` (always synced) and `menu`, layered at higher
  priority while a UI plane shows — the buttons and sticks they share go to the
  menu and gameplay reads them inactive, so movement and fire stop behind the
  menu with no downstream gate.
- Bindings are suggested for the Touch profile; the runtime resolves them for
  whatever hardware it knows.
- The right hand's aim pose is located each frame and drives weapon placement.

## Hands and gestures

- `XR_EXT_hand_tracking` is enabled when offered: 26 joints per hand, in the
  same space as the eye cameras. Hands and controllers are alternatives — the
  runtime reports whichever is live.
- Gesture recognition is the runtime's job. The ALVR client sends raw
  skeletons; recognising pinches and curls and mapping them to controller
  inputs happens streamer-side (stock ALVR: `server_core/src/hand_gestures.rs`)
  — headgasket's role in this stack. This crate consumes the result as
  ordinary action input.
