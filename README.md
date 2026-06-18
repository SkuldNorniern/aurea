# Aurea

Aurea is a pre-alpha Rust GUI toolkit for native windows, native widgets,
event-driven canvases, and renderer experiments. It provides Rust APIs over
platform-specific windowing code and a rendering layer that can target CPU and
optional GPU backends.

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

## Features

- `default`: no optional GPU backend.
- `wgpu`: enables the wgpu integration helpers.

## Building

Requirements:

- Rust 1.88 or later.
- Windows: MSVC build tools.
- macOS: Xcode command line tools.
- Linux: GTK3 development libraries.
- Mobile targets: platform SDK setup for iOS or Android. Those paths are less
  complete than desktop.

Useful checks:

```bash
cargo check
cargo test
```

## License

Apache-2.0
