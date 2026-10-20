//! ZenGPU render-to-texture rendered into an aurea window — unified graphics API, on the platform's
//! native backend (Metal on macOS, Vulkan elsewhere).

use std::{error::Error, result::Result as StdResult};
use zengpu::zsl;
use {
    aurea::{Window, WindowEvent},
    std::time::Instant,
    zengpu::{
        Acquire, Bindings, ColorAttachment, ColorTargetState, DepthState, DeviceRequest,
        FilterMode, Format, Frame, GpuAdapter, GpuDevice, GpuError, GraphicsDevice,
        GraphicsPipelineDesc, LoadOp, PresentMode, PrimitiveTopology, RenderCommands,
        RenderPassDesc, Result, SamplerDesc, Scalar, Surface, SurfaceConfig, TextureDesc,
        TextureUsage, Viewport, ViewportScissor, ZslShader,
        hal::{RasterState, TexDim},
        native::{NativeInstance, shader},
    },
};

const OFF_SIZE: u32 = 512;

const OFF_VERT_ZSL: ZslShader = zsl!(
    push Pc { angle: f32 }
    vertex vs(vi: vertex_index, pc: Pc) -> (f32x4, f32x3) {
        let pos = [f32x2(-0.6, 0.6), f32x2(0.6, 0.6), f32x2(0.0, -0.6)]
        let col = [f32x3(1.0, 0.0, 0.0), f32x3(0.0, 1.0, 0.0), f32x3(0.0, 0.0, 1.0)]
        let c = cos(pc.angle)
        let s = sin(pc.angle)
        let p = pos[vi]
        (f32x4(c * p.x - s * p.y, s * p.x + c * p.y, 0.0, 1.0), col[vi])
    }
);

const OFF_FRAG_ZSL: ZslShader = zsl!(
    fragment fs(@location(0) color: f32x3) -> f32x4 {
        color.extend(1.0)
    }
);

const SCR_VERT_ZSL: ZslShader = zsl!(
    vertex vs(vi: vertex_index) -> (f32x4, f32x2) {
        let pos = [
            f32x2(-1.0, -1.0), f32x2(1.0, -1.0), f32x2(-1.0, 1.0),
            f32x2(-1.0, 1.0), f32x2(1.0, -1.0), f32x2(1.0, 1.0),
        ]
        let p = pos[vi]
        (f32x4(p, 0.0, 1.0), p * 0.5 + 0.5)
    }
);

const SCR_FRAG_ZSL: ZslShader = zsl!(
    push Pc { tex_index: u32 }
    fragment fs(@location(0) uv: f32x2, tex: textures, pc: Pc) -> f32x4 {
        tex[pc.tex_index].sample(uv)
    }
);

fn run() -> Result<()> {
    let window = Window::new("ZenGPU — Offscreen (render-to-texture)", 800, 600)
        .map_err(|e| GpuError::Backend(format!("window: {e}")))?;

    let inst = NativeInstance::new_with_surface()?;
    let adapter = inst
        .request_graphics_adapter()
        .ok_or_else(|| GpuError::Backend("no GPU adapter found".into()))?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_graphics(DeviceRequest::default())?;

    let handles = window
        .zengpu_handles()
        .map_err(|e| GpuError::Backend(format!("window handle: {e:?}")))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Mailbox,
    };
    let surface = device.create_surface(&handles, config)?;

    let off_vert = device.create_shader(shader(&OFF_VERT_ZSL)?)?;
    let off_frag = device.create_shader(shader(&OFF_FRAG_ZSL)?)?;
    let off_pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: off_vert,
        fragment_shader: off_frag,
        vertex_layouts: &[],
        topology: PrimitiveTopology::TriangleList,
        color_targets: &[ColorTargetState {
            format: Format::Rgba8Unorm,
            blend: None,
        }],
        depth_format: None,
        depth: DepthState::default(),
        stencil: None,
        raster: RasterState::default(),
        samples: 1,
    })?;

    let scr_vert = device.create_shader(shader(&SCR_VERT_ZSL)?)?;
    let scr_frag = device.create_shader(shader(&SCR_FRAG_ZSL)?)?;
    let scr_pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: scr_vert,
        fragment_shader: scr_frag,
        vertex_layouts: &[],
        topology: PrimitiveTopology::TriangleList,
        color_targets: &[ColorTargetState {
            format: config.format,
            blend: None,
        }],
        depth_format: None,
        depth: DepthState::default(),
        stencil: None,
        raster: RasterState::default(),
        samples: 1,
    })?;

    let offscreen_tex = device.create_texture(TextureDesc {
        width: OFF_SIZE,
        height: OFF_SIZE,
        depth: 1,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
        samples: 1,
        dimension: TexDim::D2,
        mip_levels: 1,
        array_layers: 1,
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
            viewport: Viewport {
                x: 0.0,
                y: 0.0,
                width: OFF_SIZE as f32,
                height: OFF_SIZE as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            scissor: None,
        });
        list.bind(Bindings {
            scalars: &[Scalar::F32(angle)],
            ..Default::default()
        });
        list.draw(0..3, 0..1);
        list.end_render_pass();

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
        list.draw(0..6, 0..1);
        list.end_render_pass();

        surface.present(frame, list)?;
    }

    Ok(())
}

fn main() -> StdResult<(), Box<dyn Error>> {
    run()?;
    Ok(())
}
