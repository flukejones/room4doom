# vr

OpenXR stereo backend: session lifecycle, swapchain, frame pacing, controller
and hand input. macOS-only (`XR_KHR_metal_enable`).

Engine-agnostic — nothing here knows about Doom. The game consumes plain data:
per-eye cameras and regions of one combined texture, controller state, and hand
joints.

## Running with Headgasket

Activate the native runtime in Headgasket's Settings, then start Link/Dash on the
headset. From the repository root:

```sh
just vr
just vr -e 1 -m 1
```

The recipe uses the loader and `headgasket.json` installed under
`~/Library/Application Support/Headgasket VR/OpenXR`, quotes paths containing
spaces, and selects headset output with `--vr`. It explicitly selects Headgasket
even if the calling shell has another `XR_RUNTIME_JSON` value. Set
`HEADGASKET_OPENXR_DIR` for a different installation directory, or use
`just xr_runtime=/path/to/runtime.json vr` to select another runtime manifest.

`just build-vr` only builds. Running that binary directly requires `--vr` (or
`display: Headset` in the game config) and the intended `XR_RUNTIME_JSON` in its
process environment. Without a Streaming headset, the game logs the OpenXR
failure and falls back to its window.

## The chain

```
room4doom ──dlopen──▶ libopenxr_loader ──▶ Headgasket OpenXR runtime ──Mach──▶ Headgasket daemon ──Meta Link──▶ headset
```

- `OPENXR_LOADER=/path/to/libopenxr_loader.dylib` selects an explicit loader.
  Otherwise the game checks the dynamic library search path and Headgasket's
  installed loader. Load failures retain each path and native error.
- The loader selects the runtime via `XR_RUNTIME_JSON` (or the installed
  `active_runtime.json`).
- The game sees only OpenXR. Anything the device protocol carries reaches it only
  if the runtime surfaces it as an OpenXR concept: poses as spaces, buttons as
  actions, hand skeletons as `XR_EXT_hand_tracking` joints.

## Rendering

- The app enables `XR_KHR_metal_enable`, queries
  `xrGetMetalGraphicsRequirementsKHR`, and creates its command queue on the
  returned `MTLDevice`. `device.rs` matches the wgpu adapter by Metal registry ID
  and wraps the device/queue through `wgpu-hal` (wgpu is pinned).
- `Session` owns one `BGRA8Unorm_sRGB` swapchain twice the recommended eye width.
  The renderer writes its already encoded palette colours through a compatible
  `Bgra8Unorm` view of the same storage. Each frame acquires one image, and the
  two projection views cover its left and right halves.
- Each eye keeps its own scene and post-processing chain. The final post pass
  writes into that eye's viewport in the combined image without a copy or an
  application compositor.
- `xrWaitFrame` paces the frame, both eyes are located together, and
  `xrEndFrame` submits the two subimages from the same swapchain.

## Controllers

- Two action sets: `gameplay` (always synced) and `menu`, layered at higher
  priority while a UI plane shows — the buttons and sticks they share go to the
  menu and gameplay reads them inactive, so movement and fire stop behind the
  menu with no downstream gate.
- Bindings are suggested for the Touch profile; the runtime resolves them for
  whatever hardware it knows.
- `xrLocateSpaces` locates right aim, left aim, and right grip together each frame.
  Grip positions the weapon; aim directs shooting and teleportation.
- The right weapon hand has an aim pointer with either controllers or tracked
  hands. The runtime supplies its orientation; the game adds no rotation offset.
  Show Hands controls both skeletons independently.

## Movement

Headset → Movement selects Warp or Smooth locomotion. Warp has two styles:

- Aim: hold the left trigger (or left pinch), left stick forward, or right stick
  forward to show the arc; release to warp to a valid landing. Stick controls
  aim from the corresponding hand. Opening a menu, losing tracking or changing
  the control cancels the gesture; return it to rest before aiming again.
- Dash: flick the left stick in any direction to dash relative to where you
  look. Each flick makes one move; release the stick before another. Walk and
  Run distances cover one second of full movement from rest with Doom's normal
  acceleration and friction. Walls and other obstacles shorten the move.

Warp disables continuous stick walking. Smooth retains normal stick movement.
Both warp styles check player clearance and collision throughout the move.

## Hands and gestures

- `XR_EXT_hand_tracking` supplies 26 joints per active hand, in the same space as
  the eye cameras. Enable Show Hands in the game's VR settings to draw them.
- `XR_EXT_hand_interaction` bindings are suggested only when the extension is
  enabled. Put the controllers down to use gestures: right pinch fires and left
  pinch aims a warp when Aim Control is Left Trigger. Releasing the pinch
  commits a valid landing.
  Use and menu controls require controllers or the keyboard; Headgasket does not
  currently expose a grasp gesture.
