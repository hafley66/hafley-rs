# Presentation hosts over one Rust simulation

Source review only. The hosts consume immutable presentation frames; neither
host owns simulation time, rollback, collision, or animation state.

## Host boundary

```rust
struct PresentationFrame<'a> {
    confirmed: FrameId,
    transforms: &'a [RenderTransform],
    joint_matrices: &'a [Mat4; MAX_JOINTS],
    events: &'a [PresentEvent],
}

trait PresentationHost {
    fn sample_input(&mut self) -> HostInput;
    fn present(&mut self, frame: PresentationFrame<'_>);
    fn capture(&mut self, frame: FrameId) -> CaptureStatus;
}
```

The core converts host input to frame-stamped `PlayerInput`, runs fixed ticks,
handles GGRS requests, and publishes a presentation frame after resimulation.
Hosts may interpolate confirmed transforms for display, without feeding the
interpolated values back into simulation.

## Godot 4 host

Use [godot-rust `godot` 0.5.5](https://github.com/godot-rust/gdext/blob/master/godot/Cargo.toml)
through GDExtension. Its manifest exposes API levels 4.2 through 4.7,
`double-precision`, and experimental mobile/Wasm/thread features. The
[compatibility guide](https://godot-rust.github.io/book/toolchain/compatibility.html)
requires Godot 4.2 or newer and permits a runtime version at least as new as the
compiled API version. Pin the crate, Godot editor/export template, API feature,
and precision build together.

The adapter holds one Rust `Simulation`, maps Godot input callbacks into the
core input schema, and bulk-applies the published transform/joint arrays to
pre-existing scene nodes, skeleton bones, or a rendering-server buffer. Exact
bulk skeleton upload APIs and copy counts are `[UK]`; per-node setter loops are
the compatibility fallback. Godot scene import and `.ozz` conversion remain
offline asset stages. Godot's Movie Maker or deterministic screenshot sequence
can record a visible experiment, but its output is presentation evidence only.

## Second host: winit plus wgpu

Use [winit 0.30.13](https://docs.rs/winit/0.30.13/winit/) for window/input events
and [wgpu 30.0.1](https://docs.rs/wgpu/30.0.1/wgpu/) for rendering. Winit's
`ApplicationHandler` receives window/device input and redraw events. Wgpu's
`Queue::write_buffer` uploads contiguous instance and joint data, then command
buffers render it. `write_buffer` immediately copies into staging memory and
currently allocates staging memory on native targets; `StagingBelt` or explicit
mapped buffers define a reusable alternative
([Queue documentation](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html)).

The adapter owns window, surface, GPU buffers, asset textures/meshes, input
collection, and readback staging. Frame capture copies the rendered texture to
a padded GPU buffer, maps it asynchronously, and passes deterministic frame
images to an external H.264 encoder. Winit event delivery and display cadence
must not determine simulation tick count.

## Proposed cross-host test

Run the same recorded 600-frame input stream and core artifact in Godot and the
winit/wgpu host. Assert all per-tick core hashes match, confirmed frame numbers
match, each host receives identical `PresentationFrame` bytes, and no host call
mutates the core. Capture frames 0, 120, 300, and 599 plus an H.264 MP4 from
each host; compare transform/joint buffers exactly before host conversion and
compare rendered images with an explicit tolerance after conversion. Measure
adapter allocations and upload bytes. Status: **proposed, not executed**.

Unknowns are the exact Godot bulk bone/instance API at the chosen engine pin,
headless capture parity, wgpu backend-specific readback behavior, and mobile or
Wasm support for the whole dependency graph.
