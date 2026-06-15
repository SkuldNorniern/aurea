//! ZenGPU render-to-texture rendered into an aurea window — unified graphics API.
//!
//! A spinning triangle is rendered into an offscreen 512×512 color target,
//! then sampled by a fullscreen quad in the swapchain pass via the bindless
//! texture table. `ColorAttachment::sample_after` transitions the offscreen
//! target to a shader-readable layout when its pass ends. No raw `ash`/`vk`
//! types: surface, pipeline, and recording all go through
//! `zengpu_hal::{GraphicsDevice, Surface, RenderCommands}`.
//!
//! Run: cargo run --example zengpu_offscreen

use std::time::Instant;

use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu::{
    Acquire, Bindings, BlendMode, ColorAttachment, DepthState, FilterMode, Format, Frame,
    GpuAdapter, GpuDevice, GpuError, GraphicsDevice, GraphicsPipelineDesc, LoadOp, PresentMode,
    PrimitiveTopology, RenderCommands, RenderPassDesc, Result, Scalar, SamplerDesc, ShaderDesc,
    Surface, SurfaceConfig, TextureDesc, TextureUsage, VertexLayout, Viewport, ViewportScissor,
    VulkanInstance, WindowHandles,
};

const OFF_SIZE: u32 = 512;

// ── Offscreen shaders: spinning triangle ─────────────────────────────────────

const OFF_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(push_constant) uniform PC { float angle; } pc;
    layout(location = 0) out vec3 v_color;
    void main() {
        vec2 pos[3] = vec2[](vec2(-0.6, 0.6), vec2(0.6, 0.6), vec2(0.0, -0.6));
        vec3 col[3] = vec3[](vec3(1,0,0), vec3(0,1,0), vec3(0,0,1));
        float c = cos(pc.angle), s = sin(pc.angle);
        vec2 p = pos[gl_VertexIndex];
        gl_Position = vec4(c*p.x - s*p.y, s*p.x + c*p.y, 0.0, 1.0);
        v_color = col[gl_VertexIndex];
    }
    "#,
    vert,
    vulkan1_0
);

const OFF_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec3 v_color;
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(v_color, 1.0); }
    "#,
    frag,
    vulkan1_0
);

// ── Screen shaders: fullscreen quad sampling the offscreen texture ────────────

const SCR_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) out vec2 v_uv;
    void main() {
        const vec2 pos[6] = vec2[](
            vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2(-1.0,  1.0),
            vec2(-1.0,  1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0)
        );
        const vec2 uv[6] = vec2[](
            vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
            vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
        );
        gl_Position = vec4(pos[gl_VertexIndex], 0.0, 1.0);
        v_uv = uv[gl_VertexIndex];
    }
    "#,
    vert,
    vulkan1_0
);

const SCR_FRAG_SPV: &[u32] = inline_spirv!(
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
fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, std::mem::size_of_val(words)) }
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let window = Window::new("ZenGPU — Offscreen (render-to-texture)", 800, 600)
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

    // Offscreen pass: spinning triangle into a 512×512 render target.
    let off_vert = device.create_shader(ShaderDesc { spirv: spv_bytes(OFF_VERT_SPV) })?;
    let off_frag = device.create_shader(ShaderDesc { spirv: spv_bytes(OFF_FRAG_SPV) })?;
    let off_pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: off_vert,
        fragment_shader: off_frag,
        vertex_layout: VertexLayout { stride: 0, attributes: &[] },
        topology: PrimitiveTopology::TriangleList,
        color_format: Format::Rgba8Unorm,
        depth_format: None,
        depth: DepthState::default(),
        blend: BlendMode::default(),
        samples: 1,
    })?;

    // Screen pass: fullscreen quad sampling the offscreen target.
    let scr_vert = device.create_shader(ShaderDesc { spirv: spv_bytes(SCR_VERT_SPV) })?;
    let scr_frag = device.create_shader(ShaderDesc { spirv: spv_bytes(SCR_FRAG_SPV) })?;
    let scr_pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: scr_vert,
        fragment_shader: scr_frag,
        vertex_layout: VertexLayout { stride: 0, attributes: &[] },
        topology: PrimitiveTopology::TriangleList,
        color_format: config.format,
        depth_format: None,
        depth: DepthState::default(),
        blend: BlendMode::default(),
        samples: 1,
    })?;

    let offscreen_tex = device.create_texture(TextureDesc {
        width: OFF_SIZE,
        height: OFF_SIZE,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
        samples: 1,
    })?;
    let offscreen_target = device
        .register_color_target(offscreen_tex)
        .ok_or_else(|| GpuError::Backend("register_color_target: stale handle".into()))?;

    let sampler = device.create_sampler(SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        ..SamplerDesc::default()
    })?;
    let tex_index = device
        .bind_texture(offscreen_tex, sampler)
        .ok_or_else(|| GpuError::Backend("bind_texture: stale handle".into()))?;

    let start = Instant::now();

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
        let angle = start.elapsed().as_secs_f32();

        let mut list = device.create_command_list()?;

        // ── 1. Offscreen pass: spinning triangle ───────────────────────────
        list.begin_render_pass(&RenderPassDesc {
            color: &[ColorAttachment {
                target: offscreen_target,
                load: LoadOp::clear_rgb(0.05, 0.05, 0.12),
                store: true,
                sample_after: true,
            }],
            depth: None,
        });
        list.set_pipeline(off_pipeline);
        list.set_viewport_scissor(ViewportScissor {
            viewport: Viewport { x: 0.0, y: 0.0, width: OFF_SIZE as f32, height: OFF_SIZE as f32, min_depth: 0.0, max_depth: 1.0 },
            scissor: None,
        });
        list.bind(Bindings { scalars: &[Scalar::F32(angle)], ..Default::default() });
        list.draw(0..3, 0..1);
        list.end_render_pass();

        // ── 2. Screen pass: fullscreen quad sampling the offscreen target ──
        list.begin_render_pass(&RenderPassDesc {
            color: &[ColorAttachment {
                target: frame.target(),
                load: LoadOp::clear_rgb(0.0, 0.0, 0.0),
                store: true,
                sample_after: false,
            }],
            depth: None,
        });
        list.set_pipeline(scr_pipeline);
        list.set_viewport_scissor(ViewportScissor {
            viewport: Viewport { x: 0.0, y: 0.0, width: sw as f32, height: sh as f32, min_depth: 0.0, max_depth: 1.0 },
            scissor: None,
        });
        list.bind(Bindings { textures: &[tex_index], ..Default::default() });
        list.draw(0..6, 0..1);
        list.end_render_pass();

        surface.present(frame, list)?;
    }

    Ok(())
}
