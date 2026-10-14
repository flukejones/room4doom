# BSP viewer

Run the GPUI viewer after materializing the pinned dependency checkout:

```sh
just setup-gpui
cargo run -p bsp-viewer --locked -- --iwad data/doom1.wad --map E1M1
```

The `stats`, `sectors`, `diag`, and `dump` commands provide CLI diagnostics
without opening a window.

## Viewer

The GPUI module contains the window, controls, events, clipboard access and image
upload. The map model, layer drawing, selection, diagnostics, camera and moving
sectors live in `src/viewer/`. The tiny-skia 2D canvas and Software3D renderer
produce opaque RGBA frames. Texture uploads are cached until a frame changes,
and replaced images are explicitly evicted from GPUI's GPU atlas.

## Controls

- Drag to pan in 2D or look in 3D; scroll to zoom in 2D.
- Click a subsector to pin or unpin its selection.
- Cmd/Ctrl+click queries a sector. Cmd/Ctrl+drag probes a line;
  Cmd/Ctrl+Shift+drag queries a rectangle. Queries print and copy to the clipboard.
- In 3D, WASD moves, Q/E moves down/up, Shift moves faster, and clicking activates
  doors or lifts.
- The Help box above the map can expand or minimize.

## Appearance

The shared `ui-common` crate supplies the editor's named light/dark theme colours,
system appearance switching, standard Zed widgets and SVG icons. On macOS the
themed titlebar retains the native window buttons. Cmd+Q quits (Ctrl+Q elsewhere).

## Dependencies

GPUI is pinned to Zed revision `76b8ab814f0d14568063b7231905f84e82d35194`
in the workspace manifest. The workspace redirects it to `vendor/zed-gpui`.
Run `just setup-gpui` once after cloning, before any Cargo command. The script
fetches the pinned revision and applies `patches/gpui-minimal.patch`. Repeating
setup is safe. If the revision or patch changes, move or remove the generated
checkout and rerun setup. The checkout is ignored; only the patch and script
are committed.

The patch exposes individual image-codec features while preserving upstream
defaults. The GUIs disable defaults, select PNG and enable SVG for standard
widgets; SVG raster-image decoding and unused codecs remain disabled. Raw canvas
frames need no codec. Zed dependencies retain workspace version and path
inheritance. The native dependency patch also excludes `gpui_web`.
