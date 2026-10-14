# Voxel viewer

```sh
just setup-gpui
cargo run -p voxel-viewer --release -- model.kvx --slices
```

The GPUI window presents the same Software3D voxel collection and rasterization
paths used by the game. Standard Zed controls switch direct/slice rendering,
wireframe, single-slice inspection and auto-rotation. Drag rotates, arrows pan,
and scrolling zooms or steps through individual slices. S/W/D/R and +/- retain
their keyboard controls. Escape and Cmd+Q quit.

The window follows system appearance through the shared application themes and
retains native macOS titlebar buttons. Replaced frame images are explicitly
evicted from the GPU atlas.

`--no-vsync` runs the software renderer continuously on a worker for profiling,
independently of GPUI's display-paced presentation. Only the latest completed
frame is retained. The `hprof` and `render_stats` features remain available.
