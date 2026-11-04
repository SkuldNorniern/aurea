# Aurea

Pre-alpha Rust GUI toolkit: native windows and widgets, canvases, canvas
widgets in `aurea::ui`, and a renderer with CPU and optional GPU backends.

> Pre-alpha. APIs still move. Not for production.

API docs: [docs.rs/aurea](https://docs.rs/aurea)

## Quick Start

```bash
cargo add aurea
```

```rust,no_run
use aurea::elements::Label;
use aurea::{AureaResult, Window};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Hello", 400, 300)?;
    window.set_content(Label::new("Hello, Aurea!")?)?;
    window.run()
}
```

## Which widgets to use

Both kinds live in the same window, and `examples/hybrid_ui.rs` mixes them.

- **Native widgets**, `aurea::elements`: the system's own controls. They look and behave like the
  platform, and screen readers work without extra setup. Not on Android.
- **Canvas widgets**, `aurea::ui`: drawn by Aurea, the same on every platform, styled by a `Theme`.
  Mobile apps use these. Screen readers need the `accessibility` feature.

## Drawing on a canvas

A canvas redraws when asked. Its draw callback records what to paint.

```rust,no_run
use aurea::prelude::*;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Canvas", 480, 320)?;
    let canvas = Canvas::new(480, 320, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(30, 30, 30));
    canvas.set_draw_callback(|ctx| {
        let blue = Paint::new().color(Color::rgb(60, 120, 200)).style(PaintStyle::Fill);
        let white = Paint::new().color(Color::rgb(235, 235, 235)).style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(40.0, 40.0, 200.0, 120.0), &blue)?;
        ctx.draw_text("Hello", Point::new(56.0, 100.0), &white)?;
        Ok(())
    })?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()
}
```

To animate, `Canvas::on_frame` runs every frame until it returns `false`, and redraws after each.
`FrameInfo::delta` is the time to move by. It stays on a steady beat, so motion doesn't pick up
timer jitter.

## Canvas widgets

A frame draws the widgets and reads what they answer. Window events go to `UiState`, and a redraw
follows when one of them matters.

```rust,no_run
use aurea::elements::{Container, Orientation, Stack};
use aurea::render::{Canvas, RendererBackend, request_canvas_redraw};
use aurea::ui::{Button, Theme, UiState};
use aurea::{AureaResult, Rect, Window};
use std::cell::RefCell;
use std::rc::Rc;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Counter", 320, 200)?;
    let canvas = Canvas::new(320, 200, RendererBackend::Cpu)?;
    let id = canvas.id();
    let state = Rc::new(RefCell::new(UiState::<u8>::new()));
    let theme = Theme::dark();
    let mut count = 0;

    let drawn = Rc::clone(&state);
    canvas.set_draw_callback(move |ctx| {
        ctx.clear(theme.palette.background)?;
        let mut state = drawn.borrow_mut();
        let mut ui = state.frame(ctx, &theme);
        let label = format!("Clicked {count}");
        let at = Rect::new(24.0, 24.0, 160.0, 36.0);
        if Button::new(&label).show(&mut ui, 0, at)?.clicked {
            count += 1;
        }
        drop(ui);
        // Hover fades and the like ask for the next frame.
        if state.wants_frame() {
            request_canvas_redraw(id);
        }
        Ok(())
    })?;

    window.on_event(move |event| {
        if state.borrow_mut().handle(event) {
            request_canvas_redraw(id);
        }
    });

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.run()
}
```

## Threads

Windows, widgets and canvases stay on the UI thread, the one that made the first window; on macOS
and iOS that has to be the main thread. From other threads, `aurea::on_ui_thread` queues a closure and `Window::proxy` gives a `Send + Sync` handle.
In a draw callback, `aurea::after_frame` holds work that needs nothing borrowed, such as opening a
file dialog.

## Support

| | Windows | macOS | Linux (GTK3) | iOS, iPadOS | Android |
| --- | --- | --- | --- | --- | --- |
| Windows | yes | yes | yes | one full-screen host | one full-screen host |
| Native widgets | yes | yes | yes | yes, over the canvas | no |
| Native menu bar | yes | yes | yes | iPad | no |
| Canvas widgets | yes | yes | yes | yes | yes |
| IME | yes | yes | yes | yes | yes |
| Pen pressure and tilt | yes | untested on a tablet | untested on a tablet | yes | untested on a stylus |
| Fractional scaling | yes | yes | whole scales only | not needed | yes |
| File pickers | yes | yes | yes | yes | yes |
| App fonts, on canvas | yes | yes | yes | yes | yes |
| Screen readers | yes | yes | yes | no | no |

iOS and Android are experimental; their sample apps are in [`mobile/`](mobile/). Mobile apps draw
with canvas widgets. On GPU backends, blend modes other than normal draw through the CPU.

## Features

None are on by default. The CPU renderer always works.

| Feature | What it adds |
| --- | --- |
| `zengpu` | GPU rendering through ZenGPU, on Metal on macOS and Vulkan elsewhere. Experimental. |
| `wgpu` | wgpu renderer and surface helpers, for fitting Aurea into a wgpu app. |
| `accessibility` | Screen reader support for canvas widgets, through AccessKit. |
| `mobile` | iOS and Android hosts. See [`mobile/`](mobile/). |

```toml
aurea = { version = "0.1", features = ["zengpu"] }
```

Pens on Windows use Windows Ink, or Wintab when Ink sends no pen. `aurea::set_pen_mode` forces one
and `Window::pen_source` says which is live. Wintab needs no build step: the driver ships the dll.

## Memory

A canvas's memory follows its pixels: 4 bytes each, at the window's real scale. A 1920x1080 canvas
at 2x is about 32 MB per frame, and how many frames are held depends on the platform. Apps without
a canvas don't pay this. `aurea::trim_memory` frees caches that can be rebuilt.

## Examples

```bash
cargo run --example window
cargo run --example widgets
cargo run --example zengpu_canvas --features zengpu
```

| Topic | Examples |
| --- | --- |
| Windows | `window`, `multi_window`, `one_entry`, `single_instance`, `tray`, `cursor_lock` |
| Native widgets | `native_inputs`, `table`, `row_list`, `notepad`, `custom_control` |
| Canvas widgets | `widgets`, `icons`, `live_chart`, `scope`, `ide_like`, `hybrid_ui` |
| Drawing | `canvas_showcase`, `canvas_demo`, `canvas_text`, `canvas_gradient`, `canvas_image`, `canvas_blend`, `fontkit` |
| Animation | `animate_fade`, `animate_bounce`, `animate_multi` |
| ZenGPU (`--features zengpu`) | `zengpu_canvas`, `zengpu_demo`, `zengpu_triangle`, `zengpu_cube`, `zengpu_textured_quad`, `zengpu_offscreen`, `zengpu_2d_displaylist`, `zengpu_shared_context` |
| wgpu (`--features wgpu`) | `wgpu_window`, `wgpu_canvas` |

## Projects Using Aurea

| Project | Preview |
| --- | --- |
| [Ozone](https://github.com/SkuldNorniern/ozone) | ![Ozone editor](https://raw.githubusercontent.com/SkuldNorniern/ozone/main/assets/screenshots/ozone-editor.png) |

## Building

Rust 1.90+.

- **Windows:** MSVC.
- **macOS:** Xcode command line tools.
- **Linux:** GTK3 dev libraries. On Debian and Ubuntu:

  ```bash
  sudo apt install libgtk-3-dev libx11-xcb-dev pkg-config
  ```

  The `zengpu` feature also needs `libvulkan-dev`.

## License

Apache-2.0
