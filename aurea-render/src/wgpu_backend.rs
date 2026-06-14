//! wgpu peer 2D backend (feature `wgpu`).
//!
//! Implements [`Renderer`] the same way [`crate::zengpu::ZenGpuRenderer`] does:
//! draw calls are recorded into a [`DisplayList`] via [`CpuDrawingContext`],
//! then `end_frame` lowers that list to [`RenderBatches`] and uploads the
//! resulting instance arrays directly to the GPU. Where ZenGPU presents
//! through a hand-rolled Vulkan swapchain, this backend presents through a
//! `wgpu::Surface` — proving the shared-batch layer is backend-agnostic.
//!
//! The caller owns device/queue/surface creation (window-handle plumbing is a
//! root-crate concern); this module only consumes them.

use std::mem::size_of;

use wgpu::util::DeviceExt;

use crate::batch::{RectInstance, RenderBatches};
use crate::cpu::CpuDrawingContext;
use crate::display_list::DisplayList;
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::AureaResult;

const RECT_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Host-visible instance buffer that grows (doubling) to fit the largest
/// batch seen so far, reused across frames to avoid per-frame allocation.
struct InstanceBuffer {
    buffer: wgpu::Buffer,
    capacity: usize,
    elem_size: usize,
    label: &'static str,
}

impl InstanceBuffer {
    fn new(device: &wgpu::Device, label: &'static str, elem_size: usize) -> Self {
        let capacity = 1;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (elem_size * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity,
            elem_size,
            label,
        }
    }

    /// Upload `data` (a byte slice of tightly-packed instances), growing the
    /// buffer first if it can't fit.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        let needed = data.len() / self.elem_size;
        if needed > self.capacity {
            let capacity = needed.next_power_of_two();
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: (self.elem_size * capacity) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.capacity = capacity;
        }
        if !data.is_empty() {
            queue.write_buffer(&self.buffer, 0, data);
        }
    }
}

/// A [`Renderer`] that lowers the display list to GPU primitives and presents
/// them through a `wgpu::Surface`.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    viewport_buffer: wgpu::Buffer,
    rect_pipeline: wgpu::RenderPipeline,
    rect_bind_group: wgpu::BindGroup,
    rect_instances: InstanceBuffer,
    display_list: DisplayList,
    /// Reused across frames so steady-state `end_frame` does no allocation.
    batches: RenderBatches,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

impl WgpuRenderer {
    /// Wrap an already-configured `wgpu::Surface`. `config` describes the
    /// surface in physical pixels; `scale_factor` maps it to the logical size
    /// the [`DrawingContext`] draws in (matching
    /// [`CpuRasterizer`](crate::cpu::CpuRasterizer)).
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        scale_factor: f32,
    ) -> Self {
        surface.configure(&device, &config);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("aurea-wgpu2d-viewport"),
            contents: bytemuck_bytes(&[config.width as f32, config.height as f32, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("aurea-wgpu2d-viewport-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("aurea-wgpu2d-viewport-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("aurea-wgpu2d-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-rect"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });
        let rect_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-rect-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<RectInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &rect_attrs,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let rect_instances = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-rect-instances",
            size_of::<RectInstance>(),
        );

        let scale = scale_factor.max(1.0);
        Self {
            device,
            queue,
            surface,
            config: config.clone(),
            viewport_buffer,
            rect_pipeline,
            rect_bind_group: bind_group,
            rect_instances,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            logical_width: ((config.width as f32) / scale).round().max(1.0) as u32,
            logical_height: ((config.height as f32) / scale).round().max(1.0) as u32,
            scale_factor: scale,
        }
    }

    /// Surface extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}

impl Renderer for WgpuRenderer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.logical_width = width;
        self.logical_height = height;
        self.display_list.clear();
        let scale = self.scale_factor;
        self.config.width = ((width as f32 * scale).round() as u32).max(1);
        self.config.height = ((height as f32 * scale).round() as u32).max(1);
        self.surface.configure(&self.device, &self.config);
        self.queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck_bytes(&[self.config.width as f32, self.config.height as f32, 0.0, 0.0]),
        );
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        self.batches.lower_into(&self.display_list);

        let rect_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                self.batches.rects.as_ptr() as *const u8,
                self.batches.rects.len() * size_of::<RectInstance>(),
            )
        };
        self.rect_instances.upload(&self.device, &self.queue, rect_bytes);

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(aurea_foundation::AureaError::RenderingFailed);
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("aurea-wgpu2d-encoder"),
            });
        {
            let load = match self.batches.clear {
                Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                    r: c.r as f64 / 255.0,
                    g: c.g as f64 / 255.0,
                    b: c.b as f64 / 255.0,
                    a: c.a as f64 / 255.0,
                }),
                None => wgpu::LoadOp::Load,
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("aurea-wgpu2d-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if !self.batches.rects.is_empty() {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.rect_bind_group, &[]);
                pass.set_vertex_buffer(0, self.rect_instances.buffer.slice(..));
                pass.draw(0..6, 0..self.batches.rects.len() as u32);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {
        // The GPU painter redraws the full frame each present; damage is unused.
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

fn bytemuck_bytes(floats: &[f32; 4]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(floats.as_ptr() as *const u8, size_of::<[f32; 4]>()) }
}
