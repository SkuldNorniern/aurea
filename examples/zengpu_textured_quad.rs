//! ZenGPU textured quad rendered into an aurea window — unified graphics API.
//!
//! Uploads a checkerboard texture, registers it in the bindless
//! combined-image-sampler table via `VulkanDevice::bind_texture`, and samples
//! it in a fragment shader using the bindless index passed through
//! `Bindings::textures`. No raw `ash`/`vk` types: surface, pipeline, and
//! recording all go through `zengpu_hal::{GraphicsDevice, Surface, RenderCommands}`.
//!
//! Run: cargo run --example zengpu_textured_quad --features zengpu

#[cfg(feature = "zengpu")]
use aurea::{Window, WindowEvent};
#[cfg(feature = "zengpu")]
use inline_spirv::inline_spirv;
#[cfg(feature = "zengpu")]
use std::mem::size_of_val;
#[cfg(not(feature = "zengpu"))]
use std::process::exit;
#[cfg(feature = "zengpu")]
use std::slice::from_raw_parts;
use std::{error::Error, result::Result as StdResult};
#[cfg(feature = "zengpu")]
use zengpu::{
    Acquire, Bindings, BlendMode, ColorAttachment, DepthState, FilterMode, Format, Frame,
    GpuAdapter, GpuDevice, GpuError, GraphicsDevice, GraphicsPipelineDesc, LoadOp, PresentMode,
    PrimitiveTopology, RenderCommands, RenderPassDesc, Result, SamplerDesc, ShaderDesc, Surface,
    SurfaceConfig, TextureDesc, TextureUsage, Viewport, ViewportScissor, VulkanInstance,
    WindowHandles,
};

#[cfg(feature = "zengpu")]
const TEX_SIZE: u32 = 256;
#[cfg(feature = "zengpu")]
const CELL: u32 = 32;

// ── Shaders ───────────────────────────────────────────────────────────────────

#[cfg(feature = "zengpu")]
const VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) out vec2 v_uv;
    void main() {
        float x = float((gl_VertexIndex & 1) * 2);
        float y = float((gl_VertexIndex >> 1) * 2);
        v_uv = vec2(x * 0.5, y * 0.5);
        gl_Position = vec4(x - 1.0, y - 1.0, 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

#[cfg(feature = "zengpu")]
const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 1) uniform sampler2D textures[1024];
    layout(push_constant) uniform PC { uint tex_index; } pc;
    layout(location = 0) in vec2 v_uv;
    layout(location = 0) out vec4 o_color;
    void main() {
        o_color = texture(textures[pc.tex_index], v_uv);
    }
    "#,
    frag,
    vulkan1_0
);

/// View SPIR-V words as the bytes [`ShaderDesc`] expects.
#[cfg(feature = "zengpu")]
fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { from_raw_parts(words.as_ptr() as *const u8, size_of_val(words)) }
}

// ── Checkerboard texture data ─────────────────────────────────────────────────

#[cfg(feature = "zengpu")]
fn checkerboard() -> Vec<u8> {
    let mut pixels = vec![0u8; (TEX_SIZE * TEX_SIZE * 4) as usize];
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let checker = ((x / CELL) + (y / CELL)).is_multiple_of(2);
            let (r, g, b) = if checker { (220, 50, 50) } else { (30, 30, 30) };
            let i = ((y * TEX_SIZE + x) * 4) as usize;
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = 255;
        }
    }
    pixels
}

// ── Event loop ────────────────────────────────────────────────────────────────

#[cfg(feature = "zengpu")]
fn run() -> Result<()> {
    let window = Window::new("ZenGPU — Textured Quad", 800, 600)
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
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };
    let surface = device.create_surface(&handles, config)?;

    let vert_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(VERT_SPV),
    })?;
    let frag_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(FRAG_SPV),
    })?;

    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: vert_shader,
        fragment_shader: frag_shader,
        vertex_layouts: &[],
        topology: PrimitiveTopology::TriangleList,
        color_format: config.format,
        depth_format: None,
        depth: DepthState::default(),
        blend: BlendMode::default(),
        samples: 1,
    })?;

    let tex = device.create_texture(TextureDesc {
        width: TEX_SIZE,
        height: TEX_SIZE,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        samples: 1,
    })?;
    device.upload_texture_data(tex, &checkerboard())?;

    let sampler = device.create_sampler(SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Nearest,
        ..SamplerDesc::default()
    })?;

    let tex_index = device
        .bind_texture(tex, sampler)
        .ok_or_else(|| GpuError::Backend("bind_texture: stale handle".into()))?;

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

        let (sw, sh) = surface.size();

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
        list.set_viewport_scissor(ViewportScissor {
            viewport: Viewport {
                x: 0.0,
                y: 0.0,
                width: sw as f32,
                height: sh as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            scissor: None,
        });
        list.bind(Bindings {
            textures: &[tex_index],
            ..Default::default()
        });
        list.draw(0..3, 0..1);
        list.end_render_pass();

        surface.present(frame, list)?;
    }

    Ok(())
}

fn main() -> StdResult<(), Box<dyn Error>> {
    #[cfg(not(feature = "zengpu"))]
    {
        eprintln!("This example requires the `zengpu` feature.");
        eprintln!("Run with: cargo run --example zengpu_textured_quad --features zengpu");
        exit(1);
    }

    #[cfg(feature = "zengpu")]
    {
        run()?;
        Ok(())
    }
}
