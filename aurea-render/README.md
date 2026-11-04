# aurea-render

Aurea's rendering framework: a display list, the CPU rasterizer, optional GPU backends and the text
pipeline.

Apps normally use it through [`aurea::render`](https://docs.rs/aurea/latest/aurea/render/). It also
works alone: `CpuRasterizer` draws to pixels with no window.

| Feature | What it adds |
| --- | --- |
| `zengpu` | GPU backend through ZenGPU, on Metal on macOS and Vulkan elsewhere. |
| `wgpu` | wgpu backend, for fitting into a wgpu app. |

## License

Apache-2.0
