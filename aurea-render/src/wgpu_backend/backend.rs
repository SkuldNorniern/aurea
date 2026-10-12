//! wgpu 2D backend — `impl Gpu2dBackend for WgpuBackend`.
//!
//! Owns device/queue/surface bring-up, pipeline creation, GPU instance
//! buffers, and the render-pass record loop. Texture upload/eviction is
//! driven by the shared `gpu2d` core: each `upload_image` call (gradient LUT,
//! image, or text mask) creates a wgpu texture + bind group stored by slot
//! index; `FramePlan` entries carry the slot, and `present_frame` looks it up.

use crate::numeric::f32_to_u32_clamped;
use std::collections::HashMap;
use std::mem::{self, size_of};

use wgpu::util::DeviceExt;

use aurea_foundation::{AureaError, AureaResult};

use crate::batch::{CircleInstance, DrawRef, RectInstance};
use crate::capabilities::GpuFeatures;
use crate::gpu2d::{FramePlan, Gpu2dBackend, Gpu2dRenderer, ImagePlanEntry};
use crate::renderer::FrameOutcome;
use crate::types::Rect;

use super::buffer::InstanceBuffer;
use super::shaders::{CIRCLE_SHADER, GRADIENT_SHADER, IMAGE_SHADER, RECT_SHADER, TEXT_SHADER};

/// `[rect, a, b]` — 12 f32, matching the GRADIENT_SHADER `Instance` layout.
const GRADIENT_INSTANCE_STRIDE: usize = size_of::<f32>() * 12;
/// `[rect, uv, tint]` — 12 f32, matching IMAGE_SHADER.
const IMAGE_INSTANCE_STRIDE: usize = size_of::<f32>() * 12;
/// `[rect, color]` — 8 f32, matching TEXT_SHADER.
const TEXT_INSTANCE_STRIDE: usize = size_of::<f32>() * 8;

struct SlotResource {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// The texture a CPU-drawn frame is shown from, kept while its size holds.
struct CpuFrame {
    slot: u32,
    width: u32,
    height: u32,
}

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    /// Set once the surface reports itself lost; it presents nothing more
    /// until the backend is made again.
    lost: bool,
    config: wgpu::SurfaceConfiguration,
    viewport_buf: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    /// Shared bind group layout for all textured slots (gradients, images,
    /// text masks): `texture_2d<f32>` at binding 0, `sampler` at binding 1.
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    rect_pipeline: wgpu::RenderPipeline,
    circle_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    rect_buf: InstanceBuffer,
    circle_buf: InstanceBuffer,
    gradient_buf: InstanceBuffer,
    image_buf: InstanceBuffer,
    text_buf: InstanceBuffer,
    /// Instance bytes packed per frame, kept so steady state allocates
    /// nothing.
    packed: Vec<u8>,
    next_slot: u32,
    slot_resources: HashMap<u32, SlotResource>,
    cpu_frame: Option<CpuFrame>,
    cpu_plan: FramePlan,
}

/// `Gpu2dRenderer` parameterised on `WgpuBackend`.
pub type WgpuRenderer = Gpu2dRenderer<WgpuBackend>;

impl WgpuRenderer {
    /// Wrap an already-configured `wgpu::Surface`. `config` describes the
    /// surface in physical pixels; `scale_factor` maps it to the logical size
    /// the [`DrawingContext`](crate::renderer::DrawingContext) draws in.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        scale_factor: f32,
    ) -> Self {
        let scale = scale_factor.max(1.0);
        let lw = f32_to_u32_clamped((config.width as f32 / scale).round().max(1.0));
        let lh = f32_to_u32_clamped((config.height as f32 / scale).round().max(1.0));
        let backend = WgpuBackend::new(device, queue, surface, config);
        Gpu2dRenderer::from_backend(backend, lw, lh, scale)
    }

    /// Surface extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        let b = self.backend();
        (b.config.width, b.config.height)
    }
}

impl WgpuBackend {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
        surface.configure(&device, &config);
        let format = config.format;
        let init_w = config.width as f32;
        let init_h = config.height as f32;

        let viewport_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("aurea-wgpu2d-viewport"),
            contents: f32x4_bytes(&[init_w, init_h, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("aurea-wgpu2d-viewport-bg"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buf.as_entire_binding(),
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("aurea-wgpu2d-texture-layout"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("aurea-wgpu2d-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let prim_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("aurea-wgpu2d-prim-layout"),
            bind_group_layouts: &[Some(&viewport_layout)],
            immediate_size: 0,
        });
        let tex_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("aurea-wgpu2d-tex-layout"),
            bind_group_layouts: &[Some(&viewport_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-rect"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });
        let rect_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-rect-pipeline"),
            layout: Some(&prim_layout),
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let circle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-circle"),
            source: wgpu::ShaderSource::Wgsl(CIRCLE_SHADER.into()),
        });
        let circle_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let circle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-circle-pipeline"),
            layout: Some(&prim_layout),
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let textured =
            |label: &str, source: &str, attrs: &[wgpu::VertexAttribute], stride: usize| {
                textured_pipeline(&device, &tex_layout, format, (label, source), attrs, stride)
            };
        let gradient_pipeline = textured(
            "aurea-wgpu2d-gradient",
            GRADIENT_SHADER,
            &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
            GRADIENT_INSTANCE_STRIDE,
        );
        let image_pipeline = textured(
            "aurea-wgpu2d-image",
            IMAGE_SHADER,
            &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
            IMAGE_INSTANCE_STRIDE,
        );
        let text_pipeline = textured(
            "aurea-wgpu2d-text",
            TEXT_SHADER,
            &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
            TEXT_INSTANCE_STRIDE,
        );

        let rect_buf = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-rect-instances",
            size_of::<RectInstance>(),
        );
        let circle_buf = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-circle-instances",
            size_of::<CircleInstance>(),
        );
        let gradient_buf = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-gradient-instances",
            GRADIENT_INSTANCE_STRIDE,
        );
        let image_buf = InstanceBuffer::new(
            &device,
            "aurea-wgpu2d-image-instances",
            IMAGE_INSTANCE_STRIDE,
        );
        let text_buf =
            InstanceBuffer::new(&device, "aurea-wgpu2d-text-instances", TEXT_INSTANCE_STRIDE);

        Self {
            device,
            queue,
            surface,
            lost: false,
            config,
            viewport_buf,
            viewport_bind_group,
            texture_layout,
            sampler,
            rect_pipeline,
            circle_pipeline,
            gradient_pipeline,
            image_pipeline,
            text_pipeline,
            rect_buf,
            circle_buf,
            gradient_buf,
            image_buf,
            text_buf,
            packed: Vec::new(),
            next_slot: 0,
            slot_resources: HashMap::new(),
            cpu_frame: None,
            cpu_plan: FramePlan::new(),
        }
    }

    fn make_slot_resource(&self, width: u32, height: u32, rgba: &[u8]) -> SlotResource {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("aurea-wgpu2d-slot-tex"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("aurea-wgpu2d-slot-bg"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        SlotResource {
            texture,
            bind_group,
        }
    }
    /// Packs and uploads this frame's instances. The packing buffer is kept
    /// between frames.
    fn upload_instances(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        circles: &[CircleInstance],
    ) {
        self.rect_buf
            .upload(&self.device, &self.queue, cast_bytes(rects));
        self.circle_buf
            .upload(&self.device, &self.queue, cast_bytes(circles));
        // Slots are bind group keys, not vertex data, so they are left out.
        self.packed.clear();
        for g in &plan.gradients {
            for v in [&g.rect, &g.a, &g.b] {
                self.packed.extend_from_slice(cast_bytes(v.as_ref()));
            }
        }
        self.gradient_buf
            .upload(&self.device, &self.queue, &self.packed);
        self.packed.clear();
        for img in &plan.images {
            for v in [&img.rect, &img.uv, &img.tint] {
                self.packed.extend_from_slice(cast_bytes(v.as_ref()));
            }
        }
        self.image_buf
            .upload(&self.device, &self.queue, &self.packed);
        self.packed.clear();
        for t in &plan.texts {
            for v in [&t.rect, &t.color] {
                self.packed.extend_from_slice(cast_bytes(v.as_ref()));
            }
        }
        self.text_buf
            .upload(&self.device, &self.queue, &self.packed);
    }

    /// Records one draw.
    fn record(&self, pass: &mut wgpu::RenderPass<'_>, plan: &FramePlan, draw: DrawRef) {
        match draw {
            DrawRef::Rect(idx) => {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_vertex_buffer(0, self.rect_buf.buffer.slice(..));
                pass.draw(0..6, idx..idx + 1);
            }
            DrawRef::Circle(idx) => {
                pass.set_pipeline(&self.circle_pipeline);
                pass.set_vertex_buffer(0, self.circle_buf.buffer.slice(..));
                pass.draw(0..6, idx..idx + 1);
            }
            DrawRef::Gradient(idx) => {
                let slot = plan.gradients.get(idx as usize).map(|g| g.slot);
                let kind = (&self.gradient_pipeline, &self.gradient_buf);
                self.textured_draw(pass, kind, slot, idx);
            }
            DrawRef::Image(idx) => {
                let slot = plan.images.get(idx as usize).map(|i| i.slot);
                let kind = (&self.image_pipeline, &self.image_buf);
                self.textured_draw(pass, kind, slot, idx);
            }
            DrawRef::Text(idx) => {
                let slot = plan.texts.get(idx as usize).map(|t| t.slot);
                let kind = (&self.text_pipeline, &self.text_buf);
                self.textured_draw(pass, kind, slot, idx);
            }
        }
    }

    /// One instance of a pipeline that samples the texture at `slot`.
    fn textured_draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        (pipeline, instances): (&wgpu::RenderPipeline, &InstanceBuffer),
        slot: Option<u32>,
        idx: u32,
    ) {
        let Some(res) = slot.and_then(|slot| self.slot_resources.get(&slot)) else {
            return;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(1, &res.bind_group, &[]);
        pass.set_vertex_buffer(0, instances.buffer.slice(..));
        pass.draw(0..6, idx..idx + 1);
    }
}

/// Scissors the pass to `clip`, or opens it to the whole viewport. Returns
/// `false` when the clip leaves nothing to draw into.
fn set_clip(pass: &mut wgpu::RenderPass<'_>, clip: Option<Rect>, vw: u32, vh: u32) -> bool {
    let (x, y, w, h) = match clip {
        None => (0, 0, vw, vh),
        Some(r) => {
            let x0 = f32_to_u32_clamped(r.x.max(0.0).round()).min(vw);
            let y0 = f32_to_u32_clamped(r.y.max(0.0).round()).min(vh);
            let x1 = f32_to_u32_clamped((r.x + r.width).round()).min(vw);
            let y1 = f32_to_u32_clamped((r.y + r.height).round()).min(vh);
            (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
        }
    };
    if w == 0 || h == 0 {
        return false;
    }
    pass.set_scissor_rect(x, y, w, h);
    true
}

/// Writes tightly packed RGBA8 into the whole of `texture`.
fn write_rgba(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    (width, height): (u32, u32),
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// A pipeline drawing instanced quads that sample one texture, alpha
/// blended: gradients, images and text all have this shape.
fn textured_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    (label, source): (&str, &str),
    attributes: &[wgpu::VertexAttribute],
    stride: usize,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: stride as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes,
            }],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Gpu2dBackend for WgpuBackend {
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        self.config.width = physical_width.max(1);
        self.config.height = physical_height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.queue.write_buffer(
            &self.viewport_buf,
            0,
            f32x4_bytes(&[physical_width as f32, physical_height as f32, 0.0, 0.0]),
        );
        Ok(())
    }

    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        let resource = self.make_slot_resource(width, height, rgba);
        let slot = self.next_slot;
        self.next_slot += 1;
        self.slot_resources.insert(slot, resource);
        Ok(slot)
    }

    fn evict_image(&mut self, shader_slot: u32) {
        self.slot_resources.remove(&shader_slot);
    }

    fn features(&self) -> GpuFeatures {
        GpuFeatures {
            gradients: true,
            images: true,
            text: true,
            clips: true,
            ..GpuFeatures::NONE
        }
    }

    /// Keeps one texture for CPU-drawn frames and writes each new frame
    /// into it, instead of making and dropping one per frame.
    fn present_pixels(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> AureaResult<FrameOutcome> {
        let reuse = self
            .cpu_frame
            .as_ref()
            .filter(|f| f.width == width && f.height == height)
            .map(|f| f.slot);
        let slot = match reuse {
            Some(slot) => {
                if let Some(res) = self.slot_resources.get(&slot) {
                    write_rgba(&self.queue, &res.texture, (width, height), rgba);
                }
                slot
            }
            None => {
                if let Some(old) = self.cpu_frame.take() {
                    self.slot_resources.remove(&old.slot);
                }
                let slot = self.upload_image(width, height, rgba)?;
                self.cpu_frame = Some(CpuFrame {
                    slot,
                    width,
                    height,
                });
                slot
            }
        };
        let mut plan = mem::take(&mut self.cpu_plan);
        plan.reset();
        plan.viewport_width = width;
        plan.viewport_height = height;
        #[allow(clippy::cast_precision_loss)]
        plan.images.push(ImagePlanEntry {
            rect: [0.0, 0.0, width as f32, height as f32],
            uv: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
            slot,
        });
        plan.order.push(DrawRef::Image(0));
        plan.clips.push(None);
        let outcome = self.present_frame(&plan, &[], &[]);
        self.cpu_plan = plan;
        outcome
    }

    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        circles: &[CircleInstance],
    ) -> AureaResult<FrameOutcome> {
        self.upload_instances(plan, rects, circles);

        if self.lost {
            return Ok(FrameOutcome::Recreate);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(FrameOutcome::Retry);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(FrameOutcome::Retry);
            }
            // A lost surface cannot be configured back; it has to be made
            // again from its window, which is the owner's to do.
            wgpu::CurrentSurfaceTexture::Lost => {
                self.lost = true;
                return Ok(FrameOutcome::Recreate);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(AureaError::RenderingFailed);
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
            let load = match plan.clear {
                Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(c.r) / 255.0,
                    g: f64::from(c.g) / 255.0,
                    b: f64::from(c.b) / 255.0,
                    a: f64::from(c.a) / 255.0,
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

            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            for (i, draw) in plan.order.iter().enumerate() {
                let clip = plan.clips.get(i).copied().flatten();
                if !set_clip(&mut pass, clip, plan.viewport_width, plan.viewport_height) {
                    continue;
                }
                self.record(&mut pass, plan, *draw);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(FrameOutcome::Presented)
    }
}

fn f32x4_bytes(v: &[f32; 4]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, size_of::<[f32; 4]>()) }
}

fn cast_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
