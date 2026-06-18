# Aurea

Aurea is a pre-alpha Rust GUI toolkit for native windows, native widgets,
event-driven canvases, and renderer experiments. It provides Rust APIs over
platform-specific windowing code and a rendering layer that can target CPU and
optional GPU backends.

This repository also includes ongoing integration with ZenGPU. ZenGPU is an
optional Vulkan-backed rendering path used for window-level 2D rendering,
hosted canvases, shared GPU contexts, and shader/runtime work such as
ZSL-to-SPIR-V inside the nested ZenGPU checkout.

> Status: pre-alpha. The APIs, backend boundaries, and examples are still moving.
> Use this crate for experimentation and development, not production software.

## What Is In This Repository

- `aurea`: root crate with windows, menus, widgets, event polling, lifecycle
  hooks, and the public API.
- `aurea-foundation`: shared errors, platform detection, capabilities, events,
  and synchronization helpers.
- `aurea-ffi`: Rust declarations and native platform build glue.
- `aurea-render`: display lists, drawing types, CPU rasterizer, text support,
  and optional GPU backends.
- `aurea-runtime`: frame scheduling, event queues, and damage tracking.
- `aurea-animation`: small animation primitives.
- `ZenGPU`: nested ZenGPU development workspace. Aurea excludes it from the
  Aurea workspace and patches selected ZenGPU crates to this local checkout.

## Current Capabilities

- Native window backends for Windows, macOS, and Linux, with experimental iOS
  and Android platform code.
- Native widgets such as labels, buttons, boxes, split views, text fields, text
  editors, sidebars, tabs, progress bars, sliders, combo boxes, checkboxes, and
  image views. Availability still varies by platform.
- Menu bars and submenus where the platform supports them.
- Non-blocking event polling with retained callbacks for keys, mouse buttons,
  motion, wheel input, text input, focus, cursor enter/exit, raw mouse motion,
  scale-factor changes, and lifecycle events.
- Canvas rendering through Aurea's renderer abstraction, with retained draw
  callbacks, explicit invalidation, damage regions, frame scheduling, and
  per-frame animation tickers.
- CPU rasterizer with paths, rectangles, circles, images, gradients,
  blending, text measurement, tile caching, and damage-driven redraw.
- Optional `wgpu` window integration.
- Optional `zengpu` renderer path through ZenGPU and Vulkan.

## Quick Start

```bash
cargo add aurea
```

For local development from this repository:

```bash
cargo run --example window
```

```rust
use aurea::elements::Label;
use aurea::{AureaResult, Window};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Hello", 400, 300)?;
    window.set_content(Label::new("Hello, Aurea!")?)?;
    window.run()?;
    Ok(())
}
```

## Canvas Rendering

The canvas API can use Aurea's renderer abstraction for drawing inside native UI
layouts. `Canvas::set_draw_callback` is the preferred retained-mode path;
`Canvas::draw` is still available for immediate drawing. Use
`request_canvas_redraw(handle)` when a `Send + Sync` callback only has a raw
canvas handle and needs to re-run the draw callback rather than only re-blit the
cached platform buffer.

```bash
cargo run --example canvas_showcase
cargo run --example canvas_gradient
cargo run --example canvas_blend
```

The renderer layer lives in `aurea-render` and is shared by CPU and GPU paths.
Display-list drawing commands are lowered either into the CPU rasterizer or into
backend-specific GPU batches. Draw callbacks should be deterministic for the
same captured application state so the damage/tile cache can make correct reuse
decisions.

## ZenGPU Integration

ZenGPU support is opt-in:

```bash
cargo run --example zengpu_canvas --features zengpu
cargo run --example zengpu_2d_displaylist --features zengpu
cargo run --example zengpu_shared_context --features zengpu
```

The root `zengpu` feature forwards into `aurea-render/zengpu` and enables
`zengpu-hal` so `Window` can create ZenGPU window handles.

The active ZenGPU paths are:

- `Window::create_zengpu_2d()`: creates a window-level ZenGPU renderer and owns
  the window swapchain.
- `Window::create_zengpu_2d_with_context(...)`: uses a caller-owned
  `ZenGpuContext` so Aurea UI and external engine/offscreen resources can share
  one logical device.
- `Canvas::new(..., RendererBackend::ZenGpu)`: hosts a ZenGPU-backed canvas
  inside a native widget layout.

ZenGPU currently targets desktop window surfaces on Windows, macOS, and Linux
through raw-window-handle data. Linux supports XCB and Wayland handles when the
native backend can provide them.

The local ZenGPU workspace also contains compute, BLAS, SPIR-V, and ZSL crates.
Aurea currently patches only `zengpu`, `zengpu-hal`, and `zengpu-vulkan` for the
renderer path.

## Examples

Native UI and event examples:

```bash
cargo run --example window
cargo run --example multi_window
cargo run --example input_smoke
cargo run --example ide_like
cargo run --example hybrid_ui
```

Canvas examples:

```bash
cargo run --example canvas_demo
cargo run --example canvas_text
cargo run --example canvas_image
cargo run --example animate_fade
cargo run --example animate_bounce
```

ZenGPU examples:

```bash
cargo run --example zengpu_triangle --features zengpu
cargo run --example zengpu_textured_quad --features zengpu
cargo run --example zengpu_cube --features zengpu
cargo run --example zengpu_offscreen --features zengpu
```

## Features

- `default`: no optional GPU backend.
- `wgpu`: enables the wgpu integration helpers.
- `zengpu`: enables the ZenGPU renderer backend and window-level GPU surface API.

The workspace currently patches `zengpu`, `zengpu-hal`, and `zengpu-vulkan` to
the local `ZenGPU` directory. Remove or replace the `[patch.crates-io]` entries
in `Cargo.toml` when depending on published ZenGPU crates instead.

## Building

Requirements:

- Rust 1.88 or later.
- Windows: MSVC build tools.
- macOS: Xcode command line tools.
- Linux: GTK3 development libraries.
- ZenGPU examples: a Vulkan-capable system and the local `ZenGPU` checkout, or
  equivalent published crates once those are available.
- Mobile targets: platform SDK setup for iOS or Android. Those paths are less
  complete than desktop.

Useful checks:

```bash
cargo check
cargo check --features zengpu
cargo test
```

## License

Apache-2.0
