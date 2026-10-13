# Aurea

Pre-alpha Rust GUI toolkit: native windows and widgets, canvases, canvas
widgets in `aurea::ui`, and a renderer with CPU and optional GPU backends.

> Pre-alpha. APIs still move. Not for production.

Default ZenGPU canvases share one GPU device per UI thread, opened when first
needed and released when its last owner goes. `Canvas::with_gpu_context` and
`set_gpu_context` still use the application's chosen device. Renderers on the
same context also share their 2D pipelines. On Windows, compare
process RAM with shared or independent devices using:

```text
cargo run --release --features zengpu --example gpu_memory -- shared
cargo run --release --features zengpu --example gpu_memory -- isolated
cargo run --release --features zengpu --example gpu_memory -- context
```

## Quick Start

```bash
cargo add aurea
```

```rust
use aurea::elements::Label;
use aurea::{AureaResult, Window};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Hello", 400, 300)?;
    window.set_content(Label::new("Hello, Aurea!")?)?;
    window.run()
}
```

## Support

| | Windows | macOS | Linux (GTK3) |
| --- | --- | --- | --- |
| Windows, native widgets | yes | yes | yes |
| Canvas widgets | yes | yes | yes |
| IME | yes | yes | yes |
| Pen pressure and tilt | yes | not yet | not yet |

iOS and Android are experimental. On GPU backends, blend modes other than
normal draw through the CPU.

## Features

- `zengpu`: GPU rendering through ZenGPU, on Metal on macOS and Vulkan
  elsewhere. Experimental.
- `wgpu`: wgpu renderer and surface helpers.

## Examples

```bash
cargo run --example window
cargo run --example widgets
```

More in [`examples/`](examples/).

## Projects Using Aurea

| Project | Preview |
| --- | --- |
| [Ozone](https://github.com/SkuldNorniern/ozone) | ![Ozone editor](https://github.com/SkuldNorniern/ozone/blob/main/assets/screenshots/ozone-editor.png) |

## Building

Rust 1.90+. Windows: MSVC. macOS: Xcode command line tools. Linux: GTK3 dev
libraries.

## License

Apache-2.0
