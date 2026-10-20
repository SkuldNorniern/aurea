//! ZenGPU textured quad rendered into an aurea window — unified graphics API, on the platform's
//! native backend (Metal on macOS, Vulkan elsewhere).

use std::{error::Error, result::Result as StdResult};
use zengpu::zsl;
use {
    aurea::{Window, WindowEvent},
    zengpu::{
        Acquire, Bindings, ColorAttachment, ColorTargetState, DepthState, DeviceRequest,
        FilterMode, Format, Frame, GpuAdapter, GpuDevice, GpuError, GraphicsDevice,
        GraphicsPipelineDesc, LoadOp, PresentMode, PrimitiveTopology, RenderCommands,
        RenderPassDesc, Result, SamplerDesc, Surface, SurfaceConfig, TextureDesc, TextureUsage,
        Viewport, ViewportScissor, ZslShader,
        hal::{RasterState, TexDim},
        native::{NativeInstance, shader},
    },
};

const TEX_SIZE: u32 = 256;
const CELL: u32 = 32;

const VERT_ZSL: ZslShader = zsl!(
    vertex vs(vi: vertex_index) -> (f32x4, f32x2) {
        let x = f32((vi << 1) & 2)
        let y = f32(vi & 2)
        (f32x4(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0), f32x2(x, y))
    }
);

const FRAG_ZSL: ZslShader = zsl!(
    push Pc { tex_index: u32 }
    fragment fs(@location(0) uv: f32x2, tex: textures, pc: Pc) -> f32x4 {
        tex[pc.tex_index].sample(uv)
    }
);

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

fn run() -> Result<()> {
    let window = Window::new("ZenGPU — Textured Quad", 800, 600)
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

    let vert_shader = device.create_shader(shader(&VERT_ZSL)?)?;
    let frag_shader = device.create_shader(shader(&FRAG_ZSL)?)?;

    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: vert_shader,
        fragment_shader: frag_shader,
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

    let tex = device.create_texture(TextureDesc {
        width: TEX_SIZE,
        height: TEX_SIZE,
        depth: 1,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        samples: 1,
        dimension: TexDim::D2,
        mip_levels: 1,
        array_layers: 1,
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
    run()?;
    Ok(())
}
