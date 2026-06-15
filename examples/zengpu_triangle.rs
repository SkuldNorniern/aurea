//! ZenGPU triangle rendered into an aurea window — unified graphics API.
//!
//! No raw `ash`/`vk` types: surface, pipeline, and recording all go through
//! `zengpu_hal::{GraphicsDevice, Surface, RenderCommands}`.
//!
//! Run:  cargo run --example zengpu_triangle

use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu::{
    Acquire, BlendMode, ColorAttachment, DepthState, Format, Frame, GpuAdapter, GpuDevice,
    GpuError, GraphicsDevice, GraphicsPipelineDesc, LoadOp, PresentMode, PrimitiveTopology,
    RenderCommands, RenderPassDesc, Result, ShaderDesc, Surface, SurfaceConfig, VertexLayout,
    Viewport, ViewportScissor, VulkanInstance, WindowHandles,
};

const VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    void main() {
        vec2 pos[3] = vec2[](vec2(-0.5, 0.5), vec2(0.5, 0.5), vec2(0.0, -0.5));
        gl_Position = vec4(pos[gl_VertexIndex], 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(0.1, 0.6, 1.0, 1.0); }
    "#,
    frag,
    vulkan1_0
);

/// View SPIR-V words as the bytes [`ShaderDesc`] expects.
fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, std::mem::size_of_val(words)) }
}

fn main() -> Result<()> {
    let window = Window::new("ZenGPU — Triangle", 800, 600)
        .map_err(|e| GpuError::Backend(format!("window: {e}")))?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst
        .request_vulkan_adapter()
        .ok_or_else(|| GpuError::Backend("no Vulkan adapter found".into()))?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(zengpu::DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| GpuError::Backend(format!("window handle: {e:?}")))?;
    let (w, h) = window.size();
    let config = SurfaceConfig { format: Format::Bgra8Unorm, width: w, height: h, present_mode: PresentMode::Fifo };

    let surface = device.create_surface(&handles, config)?;

    let vert_shader = device.create_shader(ShaderDesc { spirv: spv_bytes(VERT_SPV) })?;
    let frag_shader = device.create_shader(ShaderDesc { spirv: spv_bytes(FRAG_SPV) })?;

    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: vert_shader,
        fragment_shader: frag_shader,
        vertex_layout: VertexLayout { stride: 0, attributes: &[] },
        topology: PrimitiveTopology::TriangleList,
        color_format: config.format,
        depth_format: None,
        depth: DepthState::default(),
        blend: BlendMode::default(),
        samples: 1,
    })?;

    'main: loop {
        for event in window.poll_events() {
            match event {
                WindowEvent::CloseRequested => break 'main,
                WindowEvent::Resized { width, height } => {
                    let _ = surface.resize(width.max(1), height.max(1));
                }
                _ => {}
            }
        }

        let frame = match surface.acquire()? {
            Acquire::Frame(frame) => frame,
            Acquire::Skip => continue,
        };

        let mut list = device.create_command_list()?;
        list.begin_render_pass(&RenderPassDesc {
            color: &[ColorAttachment {
                target: frame.target(),
                load: LoadOp::clear_rgb(0.02, 0.02, 0.02),
                store: true,
                sample_after: false,
            }],
            depth: None,
        });
        list.set_pipeline(pipeline);
        let (sw, sh) = surface.size();
        list.set_viewport_scissor(ViewportScissor {
            viewport: Viewport { x: 0.0, y: 0.0, width: sw as f32, height: sh as f32, min_depth: 0.0, max_depth: 1.0 },
            scissor: None,
        });
        list.draw(0..3, 0..1);
        list.end_render_pass();

        surface.present(frame, list)?;
    }

    Ok(())
}
