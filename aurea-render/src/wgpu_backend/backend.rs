//! wgpu 2D backend — `impl Gpu2dBackend for WgpuBackend`.
//!
//! Owns device/queue/surface bring-up, pipeline creation, GPU instance
//! buffers, and the render-pass record loop. Texture upload/eviction is
//! driven by the shared `gpu2d` core: each `upload_image` call (gradient LUT,
//! image, or text mask) creates a wgpu texture + bind group stored by slot
//! index; `FramePlan` entries carry the slot, and `present_frame` looks it up.

use std::collections::HashMap;
use std::mem::size_of;

use wgpu::util::DeviceExt;

use aurea_foundation::{AureaError, AureaResult};

use crate::batch::{CircleInstance, DrawRef, RectInstance};
use crate::gpu2d::{FramePlan, Gpu2dBackend, Gpu2dRenderer};

use super::buffer::InstanceBuffer;
use super::shaders::{CIRCLE_SHADER, GRADIENT_SHADER, RECT_SHADER};

/// `[rect, a, b]` — 12 f32, matching the GRADIENT_SHADER `Instance` layout.
const GRADIENT_INSTANCE_STRIDE: usize = size_of::<f32>() * 12;

struct SlotResource {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
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
    rect_buf: InstanceBuffer,
    circle_buf: InstanceBuffer,
    gradient_buf: InstanceBuffer,
    next_slot: u32,
    slot_resources: HashMap<u32, SlotResource>,
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
        let lw = ((config.width as f32) / scale).round().max(1.0) as u32;
        let lh = ((config.height as f32) / scale).round().max(1.0) as u32;
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

        let gradient_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("aurea-wgpu2d-gradient"),
            source: wgpu::ShaderSource::Wgsl(GRADIENT_SHADER.into()),
        });
        let gradient_attrs =
            wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let gradient_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aurea-wgpu2d-gradient-pipeline"),
            layout: Some(&tex_layout),
            vertex: wgpu::VertexState {
                module: &gradient_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: GRADIENT_INSTANCE_STRIDE as wgpu::BufferAddress,
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

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

        Self {
            device,
            queue,
            surface,
            config,
            viewport_buf,
            viewport_bind_group,
            texture_layout,
            sampler,
            rect_pipeline,
            circle_pipeline,
            gradient_pipeline,
            rect_buf,
            circle_buf,
            gradient_buf,
            next_slot: 0,
            slot_resources: HashMap::new(),
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
            _texture: texture,
            bind_group,
        }
    }
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

    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        circles: &[CircleInstance],
    ) -> AureaResult<()> {
        // Pack gradient instance bytes ([rect, a, b] only — slot is the bind
        // group key, not sent to the vertex shader).
        let mut gradient_bytes =
            Vec::with_capacity(plan.gradients.len() * GRADIENT_INSTANCE_STRIDE);
        for g in &plan.gradients {
            gradient_bytes.extend_from_slice(cast_bytes(g.rect.as_ref()));
            gradient_bytes.extend_from_slice(cast_bytes(g.a.as_ref()));
            gradient_bytes.extend_from_slice(cast_bytes(g.b.as_ref()));
        }

        self.rect_buf
            .upload(&self.device, &self.queue, cast_bytes(rects));
        self.circle_buf
            .upload(&self.device, &self.queue, cast_bytes(circles));
        self.gradient_buf
            .upload(&self.device, &self.queue, &gradient_bytes);

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
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

            pass.set_bind_group(0, &self.viewport_bind_group, &[]);

            for draw in &plan.order {
                match *draw {
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
                        if let Some(slot) = plan.gradients.get(idx as usize).map(|g| g.slot)
                            && let Some(res) = self.slot_resources.get(&slot)
                        {
                            pass.set_pipeline(&self.gradient_pipeline);
                            pass.set_bind_group(1, &res.bind_group, &[]);
                            pass.set_vertex_buffer(0, self.gradient_buf.buffer.slice(..));
                            pass.draw(0..6, idx..idx + 1);
                        }
                    }
                    // Image and text pipelines land in a later pass.
                    DrawRef::Image(_) | DrawRef::Text(_) => {}
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn f32x4_bytes(v: &[f32; 4]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, size_of::<[f32; 4]>()) }
}

fn cast_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
