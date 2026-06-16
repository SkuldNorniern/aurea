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

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

use wgpu::util::DeviceExt;

use crate::batch::{CircleInstance, DrawRef, RectInstance, RenderBatches};
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

/// Expands each instance's bounding-box quad and evaluates a signed distance
/// field in the fragment shader for a 1px-antialiased edge, matching
/// `ZenGpuRenderer`'s circle pipeline.
const CIRCLE_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let r = instance.center_radius.z;
    let px = instance.center_radius.xy + corner * r;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.local = corner * r;
    out.color = instance.color;
    out.radius = r;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dist = length(in.local);
    let alpha = 1.0 - smoothstep(in.radius - 1.0, in.radius, dist);
    if (alpha <= 0.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Computes a gradient parameter `t` per pixel and samples a 256x1 LUT
/// texture (group 1), matching `ZenGpuRenderer`'s gradient pipeline.
const GRADIENT_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var lut_tex: texture_2d<f32>;
@group(1) @binding(1) var lut_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) a: vec4<f32>,
    @location(2) b: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) a: vec4<f32>,
    @location(2) b: vec4<f32>,
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
    out.px = px;
    out.a = instance.a;
    out.b = instance.b;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var t: f32;
    if (in.a.w < 0.5) {
        let d = in.b.xy - in.a.xy;
        t = dot(in.px - in.a.xy, d) / max(dot(d, d), 1e-6);
    } else {
        t = length(in.px - in.a.xy) / max(in.a.z, 1e-6);
    }
    let u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0;
    return textureSample(lut_tex, lut_sampler, vec2<f32>(u, 0.5));
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
    circle_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    rect_bind_group: wgpu::BindGroup,
    gradient_bind_group_layout: wgpu::BindGroupLayout,
    gradient_sampler: wgpu::Sampler,
    rect_instances: InstanceBuffer,
    circle_instances: InstanceBuffer,
    gradient_instances: InstanceBuffer,
    /// LUT texture + bind group per distinct 256x1 gradient LUT, keyed by a
    /// content hash of the LUT bytes. Grows unboundedly across frames; no
    /// eviction yet (see status.md watch-outs).
    gradient_lut_textures: HashMap<u64, (wgpu::Texture, wgpu::BindGroup)>,
    /// LUT hash keys for `batches.gradients`, rebuilt each frame.
    gradient_keys: Vec<u64>,
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

        let circle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-circle"),
            source: wgpu::ShaderSource::Wgsl(CIRCLE_SHADER.into()),
        });
        let circle_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let circle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-circle-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &circle_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<CircleInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &circle_attrs,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &circle_shader,
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
        let circle_instances = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-circle-instances",
            size_of::<CircleInstance>(),
        );

        let gradient_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("aurea-wgpu2d-gradient-lut-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let gradient_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("aurea-wgpu2d-gradient-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let gradient_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("aurea-wgpu2d-gradient-pipeline-layout"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&gradient_bind_group_layout)],
                immediate_size: 0,
            });
        let gradient_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-gradient"),
            source: wgpu::ShaderSource::Wgsl(GRADIENT_SHADER.into()),
        });
        let gradient_attrs =
            wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let gradient_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-gradient-pipeline"),
            layout: Some(&gradient_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &gradient_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: (size_of::<f32>() * 12) as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &gradient_attrs,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gradient_shader,
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
        let gradient_instances = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-gradient-instances",
            GRADIENT_INSTANCE_SIZE,
        );

        let scale = scale_factor.max(1.0);
        Self {
            device,
            queue,
            surface,
            config: config.clone(),
            viewport_buffer,
            rect_pipeline,
            circle_pipeline,
            gradient_pipeline,
            rect_bind_group: bind_group,
            gradient_bind_group_layout,
            gradient_sampler,
            rect_instances,
            circle_instances,
            gradient_instances,
            gradient_lut_textures: HashMap::new(),
            gradient_keys: Vec::new(),
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
            bytemuck_bytes(&[
                self.config.width as f32,
                self.config.height as f32,
                0.0,
                0.0,
            ]),
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
        self.rect_instances
            .upload(&self.device, &self.queue, rect_bytes);

        let circle_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                self.batches.circles.as_ptr() as *const u8,
                self.batches.circles.len() * size_of::<CircleInstance>(),
            )
        };
        self.circle_instances
            .upload(&self.device, &self.queue, circle_bytes);

        self.gradient_keys.clear();
        let mut gradient_bytes =
            Vec::with_capacity(self.batches.gradients.len() * GRADIENT_INSTANCE_SIZE);
        for gradient in &self.batches.gradients {
            let key = lut_hash_key(&gradient.lut);
            self.gradient_keys.push(key);
            gradient_bytes.extend_from_slice(bytemuck_bytes(&gradient.rect));
            gradient_bytes.extend_from_slice(bytemuck_bytes(&gradient.a));
            gradient_bytes.extend_from_slice(bytemuck_bytes(&gradient.b));
            self.gradient_lut_textures.entry(key).or_insert_with(|| {
                create_gradient_lut_texture(
                    &self.device,
                    &self.queue,
                    &self.gradient_bind_group_layout,
                    &self.gradient_sampler,
                    &gradient.lut,
                )
            });
        }
        self.gradient_instances
            .upload(&self.device, &self.queue, &gradient_bytes);

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

            pass.set_bind_group(0, &self.rect_bind_group, &[]);
            for draw in &self.batches.order {
                match *draw {
                    DrawRef::Rect(index) => {
                        pass.set_pipeline(&self.rect_pipeline);
                        pass.set_vertex_buffer(0, self.rect_instances.buffer.slice(..));
                        pass.draw(0..6, index..index + 1);
                    }
                    DrawRef::Circle(index) => {
                        pass.set_pipeline(&self.circle_pipeline);
                        pass.set_vertex_buffer(0, self.circle_instances.buffer.slice(..));
                        pass.draw(0..6, index..index + 1);
                    }
                    DrawRef::Gradient(index) => {
                        let key = self.gradient_keys[index as usize];
                        let (_, lut_bind_group) = &self.gradient_lut_textures[&key];
                        pass.set_pipeline(&self.gradient_pipeline);
                        pass.set_bind_group(1, lut_bind_group, &[]);
                        pass.set_vertex_buffer(0, self.gradient_instances.buffer.slice(..));
                        pass.draw(0..6, index..index + 1);
                    }
                    // Image/text pipelines land in later P7-O stages.
                    DrawRef::Image(_) | DrawRef::Text(_) => {}
                }
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

/// `[rect, a, b]`, each `[f32; 4]` — the packed `GradientInstance` fields
/// uploaded to `gradient_instances` (the `lut` field is a separate texture).
const GRADIENT_INSTANCE_SIZE: usize = size_of::<f32>() * 12;

/// Content hash of a gradient's 256x1 LUT bytes, used to key
/// `gradient_lut_textures` so identical gradients share one texture.
fn lut_hash_key(lut: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    lut.hash(&mut hasher);
    hasher.finish()
}

/// Upload a 256x1 RGBA8 gradient LUT as a texture and build its bind group
/// (group 1: texture + sampler) for the gradient pipeline.
fn create_gradient_lut_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    lut: &[u8],
) -> (wgpu::Texture, wgpu::BindGroup) {
    let size = wgpu::Extent3d {
        width: 256,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("aurea-wgpu2d-gradient-lut"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        lut,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256 * 4),
            rows_per_image: Some(1),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("aurea-wgpu2d-gradient-lut-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (texture, bind_group)
}
