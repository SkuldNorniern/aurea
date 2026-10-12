//! ZenGPU triangle rendered into an aurea window — unified graphics API.
//!
//! No raw `ash`/`vk` types: surface, pipeline, and recording all go through
//! `zengpu_hal::{GraphicsDevice, Surface, RenderCommands}`.
//!
//! Run:  cargo run --example zengpu_triangle --features zengpu

use std::{error::Error, result::Result as StdResult};
use zengpu::zsl;
use {
    aurea::{Window, WindowEvent},
    std::{mem::size_of_val, slice::from_raw_parts},
    zengpu::{
        Acquire, ColorAttachment, ColorTargetState, DepthState, DeviceRequest, Format, Frame,
        GpuAdapter, GpuDevice, GpuError, GraphicsDevice, GraphicsPipelineDesc, LoadOp, PresentMode,
        PrimitiveTopology, RenderCommands, RenderPassDesc, Result, ShaderDesc, Surface,
        SurfaceConfig, Viewport, ViewportScissor, VulkanInstance, hal::RasterState,
    },
};

const VERT_SPV: &[u32] = zsl!(
    vertex vs(vi: vertex_index) -> f32x4 {
        let pos = [f32x2(-0.5, 0.5), f32x2(0.5, 0.5), f32x2(0.0, -0.5)]
        f32x4(pos[vi], 0.0, 1.0)
    }
)
.spv;

const FRAG_SPV: &[u32] = zsl!(
    fragment fs() -> f32x4 {
        f32x4(0.1, 0.6, 1.0, 1.0)
    }
)
.spv;

/// View SPIR-V words as the bytes [`ShaderDesc`] expects.
fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { from_raw_parts(words.as_ptr() as *const u8, size_of_val(words)) }
}

fn run() -> Result<()> {
    let window = Window::new("ZenGPU — Triangle", 800, 600)
        .map_err(|e| GpuError::Backend(format!("window: {e}")))?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst
        .request_vulkan_adapter()
        .ok_or_else(|| GpuError::Backend("no Vulkan adapter found".into()))?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

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

    let vert_shader = device.create_shader(ShaderDesc::spirv(spv_bytes(VERT_SPV)))?;
    let frag_shader = device.create_shader(ShaderDesc::spirv(spv_bytes(FRAG_SPV)))?;

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
