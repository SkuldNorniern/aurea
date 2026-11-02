# Aurea

Pre-alpha Rust GUI toolkit: native windows and widgets, canvases, canvas
widgets in `aurea::ui`, and a renderer with CPU and optional GPU backends.

> Pre-alpha. APIs still move. Not for production.

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

| | Windows | macOS | Linux (GTK3) | iOS, iPadOS | Android |
| --- | --- | --- | --- | --- | --- |
| Windows | yes | yes | yes | one full-screen host | one full-screen host |
| Native widgets | yes | yes | yes | some | no |
| Native menu bar | yes | yes | yes | no | no |
| Canvas widgets | yes | yes | yes | yes | yes |
| IME | yes | yes | yes | yes | yes |
| Pen pressure and tilt | yes | untested on a tablet | untested on a tablet | yes | untested on a stylus |
| Fractional scaling | yes | yes | whole scales only | not needed | yes |
| File pickers | yes | yes | yes | yes | no |
| Screen readers | yes | yes | yes | no | no |

iOS and Android are experimental; their sample apps are in [`mobile/`](mobile/). Mobile apps draw
with canvas widgets. On GPU backends, blend modes other than normal draw through the CPU.

## Features

- `zengpu`: GPU rendering through ZenGPU, on Metal on macOS and Vulkan
  elsewhere. Experimental.
- `wgpu`: wgpu renderer and surface helpers.
- `accessibility`: screen reader support for canvas widgets, through AccessKit.
- `mobile`: iOS and Android hosts.

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
