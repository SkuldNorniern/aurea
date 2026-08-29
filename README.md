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
- CPU rasterizer with paths, rectangles, circles, images, gradients, blending,
  text measurement, tile caching, and damage-driven redraw. Fills and strokes
  are antialiased through the same coverage path.
- Plots and oscilloscope views in `aurea::render::graph`, drawn onto a canvas:
  linear and log axes, tick and grid generation, line, step, point, area and
  bar series, cursors, and a scope with channels, timebase and trigger.
- Optional GPU acceleration through the in-house ZenGPU backend.
- Optional `wgpu` window integration.

## Threads

Windows, canvases and widgets belong to the thread that brought the platform
up. `Window` is neither `Send` nor `Sync`, so the compiler stops them from
travelling; native window systems are thread-affine and a window that could
move between threads would be a standing invitation to undefined behaviour.

To reach the UI from another thread, use a proxy. Work queued on it runs the
next time the window pumps.

```rust,no_run
use aurea::Window;

# fn main() -> aurea::AureaResult<()> {
let window = Window::new("App", 800, 600)?;
let proxy = window.proxy();

std::thread::spawn(move || {
    proxy.dispatch(|window| {
        let _ = window.set_title("done");
    });
});

window.run()
# }
```

Event and widget callbacks run on the UI thread and are not required to be
`Send` or `Sync`, so they can capture windows and widgets directly.

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
backend-specific GPU batches.

A draw callback runs every frame the canvas is dirty and rebuilds the whole
display list; what makes that cheap is the diff underneath it. Each item carries
a cache key covering everything that affects its pixels, the list is compared
against the previous frame's, and only the tiles that changed are repainted —
with every draw clipped to that region, so an item spanning changed and
unchanged tiles only repaints the changed part.

Two things follow for callers. Draw callbacks must be deterministic for the same
application state: one that varies with wall-clock time or hash iteration order
will produce a display list that hashes differently every frame and defeat the
diff. And identical output should be produced identically — the same shape drawn
at the same place gets the same key whether it got there through a transform or
directly, so either is fine, but recomputing geometry in a way that jitters by a
fraction of a pixel is not.

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

## Projects Using Aurea

| Project | Preview |
| --- | --- |
| [Ozone](https://github.com/SkuldNorniern/ozone) | ![Ozone editor](https://github.com/SkuldNorniern/ozone/blob/main/assets/screenshots/ozone-editor.png) |

## What Is Supported

What each part of Aurea is expected to do, so that "experimental" means
something specific rather than a disclaimer on the whole project.

| Part | Status |
| --- | --- |
| Windows, macOS, Linux/GTK3 | Supported |
| CPU rasterizer | Supported — the reference backend, and what correctness is measured against |
| Native widgets | Supported, a subset that varies by platform |
| Graph and scope | Supported |
| Window lifetime, input, clipboard | Supported |
| ZenGPU renderer | Experimental — Aurea's primary GPU backend, and where accelerated work goes |
| wgpu renderer | Experimental — a compatibility path, not the architectural target |
| External wgpu surface integration | Supported, for fitting Aurea into an application already built on wgpu |
| iOS, Android | Experimental, not a 0.1 compatibility target |
| Arbitrary affine transforms on a canvas | Partial; see the limits below |
| Complex-script text shaping | Not yet |
| Accessibility for canvas-drawn UI | Not yet |

Known limits worth reading before relying on them:

- The GPU backends draw a subset of what the CPU rasterizer does. Paths,
  blend modes other than `Normal`, and the older text commands are not
  lowered; a frame reports how many draws it left out rather than pretending
  otherwise. A canvas needing them wants the CPU renderer.
- Rotation and skew move text and images but do not rotate the glyphs or the
  image itself, and a circle stays a circle under a non-uniform scale.
- A retained draw callback is re-recorded on every frame that is redrawn.
  Damage tracking keeps that off the screen, not off the CPU.

## Features

- `default`: no optional GPU backend.
- `zengpu`: GPU-accelerated rendering through ZenGPU, Aurea's primary
  accelerated backend. Experimental: it does not yet match the CPU
  rasterizer's command coverage or partial repaint.
- `wgpu`: the wgpu integration helpers and compatibility renderer, for
  applications already built around wgpu. Not Aurea's primary GPU backend.

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
