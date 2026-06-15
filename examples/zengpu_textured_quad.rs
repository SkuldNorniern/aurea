//! ZenGPU textured quad — user-side surface with bindless descriptor indexing.
//!
//! Uploads a checkerboard texture via the HAL device, then renders it fullscreen
//! using a bindless array of 64 combined image samplers. The render pass / pipeline /
//! descriptor sets all live here; `zengpu-vulkan` owns only swapchain + sync.
//!
//! Run:  cargo run --example zengpu_textured_quad

use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu::vulkan::{ash, vk};
use zengpu::{
    BeginFrame, DeviceContext, DeviceRequest, FilterMode, Format, GpuAdapter, GpuDevice,
    PresentMode, Result, SamplerDesc, SurfaceConfig, Swapchain, TextureDesc, TextureHandle,
    TextureUsage, VulkanDevice, VulkanInstance, WindowHandles,
};

const W: u32 = 800;
const H: u32 = 600;
const TEX_SIZE: u32 = 256;
const CELL: u32 = 32;
const BINDLESS: u32 = 64;
const MAX_FRAMES: usize = 2;

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

const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 0) uniform sampler2D textures[64];
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

// ── Placeholder (fills unused bindless slots) ─────────────────────────────────

struct Placeholder {
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
    sampler: vk::Sampler,
}

unsafe impl Send for Placeholder {}
unsafe impl Sync for Placeholder {}

fn make_placeholder(ctx: &DeviceContext) -> Result<Placeholder> {
    let dev = ctx.device();
    let mem_props = ctx.memory_properties();

    let image = unsafe {
        dev.create_image(
            &vk::ImageCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                format: vk::Format::R8G8B8A8_UNORM,
                extent: vk::Extent3D { width: 1, height: 1, depth: 1 },
                mip_levels: 1,
                array_layers: 1,
                samples: vk::SampleCountFlags::TYPE_1,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                initial_layout: vk::ImageLayout::UNDEFINED,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| zengpu_hal::GpuError::Backend(format!("placeholder vkCreateImage: {e}")))?
    };

    let mem_reqs = unsafe { dev.get_image_memory_requirements(image) };
    let type_index = (0..mem_props.memory_type_count)
        .find(|&i| {
            mem_reqs.memory_type_bits & (1 << i) != 0
                && mem_props.memory_types[i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .ok_or_else(|| {
            unsafe { dev.destroy_image(image, None) };
            zengpu_hal::GpuError::Backend("no device-local memory for placeholder".to_string())
        })?;

    let memory = unsafe {
        dev.allocate_memory(
            &vk::MemoryAllocateInfo {
                allocation_size: mem_reqs.size,
                memory_type_index: type_index,
                ..Default::default()
            },
            None,
        )
        .map_err(|_| {
            dev.destroy_image(image, None);
            zengpu_hal::GpuError::Backend("placeholder OOM".to_string())
        })?
    };

    unsafe {
        dev.bind_image_memory(image, memory, 0).map_err(|e| {
            dev.destroy_image(image, None);
            dev.free_memory(memory, None);
            zengpu_hal::GpuError::Backend(format!("placeholder bind: {e}"))
        })?
    };

    let view = unsafe {
        dev.create_image_view(
            &vk::ImageViewCreateInfo {
                image,
                view_type: vk::ImageViewType::TYPE_2D,
                format: vk::Format::R8G8B8A8_UNORM,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                ..Default::default()
            },
            None,
        )
        .map_err(|e| {
            dev.destroy_image(image, None);
            dev.free_memory(memory, None);
            zengpu_hal::GpuError::Backend(format!("placeholder view: {e}"))
        })?
    };

    let sampler = unsafe {
        dev.create_sampler(
            &vk::SamplerCreateInfo {
                mag_filter: vk::Filter::NEAREST,
                min_filter: vk::Filter::NEAREST,
                address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| {
            dev.destroy_image_view(view, None);
            dev.destroy_image(image, None);
            dev.free_memory(memory, None);
            zengpu_hal::GpuError::Backend(format!("placeholder sampler: {e}"))
        })?
    };

    // Transition to SHADER_READ_ONLY using a white clear (no staging needed — this
    // 1×1 image only needs to read as white, so we clear it instead of uploading).
    ctx.one_shot_submit(|dev, cmd| {
        unsafe {
            dev.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    src_access_mask: vk::AccessFlags::empty(),
                    dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    ..Default::default()
                }],
            );
            dev.cmd_clear_color_image(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [1.0, 1.0, 1.0, 1.0] },
                &[vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                }],
            );
            dev.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    ..Default::default()
                }],
            );
        }
        Ok(())
    })?;

    Ok(Placeholder { image, view, memory, sampler })
}

// ── TexturedSurface ───────────────────────────────────────────────────────────

struct TexturedSurface {
    ctx: DeviceContext,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    placeholder: Placeholder,
    sc: Swapchain,
}

impl TexturedSurface {
    fn new(
        device: &VulkanDevice,
        handles: &WindowHandles,
        config: SurfaceConfig,
        tex_view: vk::ImageView,
        samp_vk: vk::Sampler,
    ) -> Result<Self> {
        let sc = Swapchain::new(device, handles, config, MAX_FRAMES)?;
        let ctx = sc.context();
        let dev = ctx.device();

        let render_pass = make_render_pass(dev, sc.format())?;
        let framebuffers = make_framebuffers(dev, render_pass, &sc.image_views(), sc.extent())?;
        let (descriptor_pool, descriptor_set_layout, descriptor_set) =
            make_bindless_descriptors(dev)?;
        let (pipeline_layout, pipeline) =
            make_pipeline(dev, render_pass, descriptor_set_layout)?;

        let placeholder = make_placeholder(&ctx)?;
        fill_bindless_slots(dev, descriptor_set, placeholder.view, placeholder.sampler);
        update_bindless_slot(dev, descriptor_set, 0, tex_view, samp_vk);

        Ok(Self {
            ctx,
            render_pass,
            framebuffers,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            pipeline_layout,
            pipeline,
            placeholder,
            sc,
        })
    }

    fn rebuild(&mut self) -> Result<()> {
        let dev = self.ctx.device();
        unsafe {
            for &fb in &self.framebuffers {
                dev.destroy_framebuffer(fb, None);
            }
        }
        self.framebuffers =
            make_framebuffers(dev, self.render_pass, &self.sc.image_views(), self.sc.extent())?;
        Ok(())
    }

    fn present(&mut self) -> Result<()> {
        let bf = self.sc.begin_frame()?;
        let (current, index) = match bf {
            BeginFrame::Image { current, index } => (current, index),
            BeginFrame::Recreated => return self.rebuild(),
            BeginFrame::Skip => return Ok(()),
        };

        let cmd = self.sc.cmd_buffer(current);
        let dev = self.ctx.device();
        let extent = self.sc.extent();
        unsafe {
            dev.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .map_err(|e| zengpu_hal::GpuError::Backend(format!("reset_command_buffer: {e}")))?;
            dev.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| zengpu_hal::GpuError::Backend(format!("begin_command_buffer: {e}")))?;

            let clear = vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.02, 0.02, 0.02, 1.0] },
            };
            dev.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo {
                    render_pass: self.render_pass,
                    framebuffer: self.framebuffers[index as usize],
                    render_area: vk::Rect2D { offset: vk::Offset2D::default(), extent },
                    clear_value_count: 1,
                    p_clear_values: &clear,
                    ..Default::default()
                },
                vk::SubpassContents::INLINE,
            );
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0, y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }]);
            dev.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D::default(), extent }],
            );
            dev.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            dev.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                &0u32.to_ne_bytes(),
            );
            dev.cmd_draw(cmd, 3, 1, 0, 0);
            dev.cmd_end_render_pass(cmd);
            dev.end_command_buffer(cmd)
                .map_err(|e| zengpu_hal::GpuError::Backend(format!("end_command_buffer: {e}")))?;
        }

        if self.sc.end_frame(&bf, cmd)? {
            self.rebuild()?;
        }
        Ok(())
    }

    fn resize(&mut self, w: u32, h: u32) -> Result<()> {
        self.sc.resize(w, h)?;
        self.rebuild()
    }
}

impl Drop for TexturedSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ctx.device().device_wait_idle();
            let dev = self.ctx.device();
            dev.destroy_pipeline(self.pipeline, None);
            dev.destroy_pipeline_layout(self.pipeline_layout, None);
            dev.destroy_descriptor_pool(self.descriptor_pool, None);
            dev.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            dev.destroy_sampler(self.placeholder.sampler, None);
            dev.destroy_image_view(self.placeholder.view, None);
            dev.destroy_image(self.placeholder.image, None);
            dev.free_memory(self.placeholder.memory, None);
            for &fb in &self.framebuffers {
                dev.destroy_framebuffer(fb, None);
            }
            dev.destroy_render_pass(self.render_pass, None);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_render_pass(dev: &ash::Device, format: vk::Format) -> Result<vk::RenderPass> {
    let attachment = vk::AttachmentDescription {
        format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
        ..Default::default()
    };
    let color_ref =
        vk::AttachmentReference { attachment: 0, layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL };
    let subpass = vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
        color_attachment_count: 1,
        p_color_attachments: &color_ref,
        ..Default::default()
    };
    let dep = vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        ..Default::default()
    };
    unsafe {
        dev.create_render_pass(
            &vk::RenderPassCreateInfo {
                attachment_count: 1,
                p_attachments: &attachment,
                subpass_count: 1,
                p_subpasses: &subpass,
                dependency_count: 1,
                p_dependencies: &dep,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| zengpu_hal::GpuError::Backend(format!("create_render_pass: {e}")))
    }
}

fn make_framebuffers(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    views
        .iter()
        .map(|&view| {
            let atts = [view];
            unsafe {
                dev.create_framebuffer(
                    &vk::FramebufferCreateInfo {
                        render_pass,
                        attachment_count: 1,
                        p_attachments: atts.as_ptr(),
                        width: extent.width,
                        height: extent.height,
                        layers: 1,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| zengpu_hal::GpuError::Backend(format!("create_framebuffer: {e}")))
            }
        })
        .collect()
}

fn make_bindless_descriptors(
    dev: &ash::Device,
) -> Result<(vk::DescriptorPool, vk::DescriptorSetLayout, vk::DescriptorSet)> {
    let binding = vk::DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: BINDLESS,
        stage_flags: vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let layout = unsafe {
        dev.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo {
                binding_count: 1,
                p_bindings: &binding,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| {
            zengpu_hal::GpuError::Backend(format!("create_descriptor_set_layout: {e}"))
        })?
    };
    let pool_size =
        vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: BINDLESS };
    let pool = unsafe {
        dev.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo {
                max_sets: 1,
                pool_size_count: 1,
                p_pool_sizes: &pool_size,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| {
            dev.destroy_descriptor_set_layout(layout, None);
            zengpu_hal::GpuError::Backend(format!("create_descriptor_pool: {e}"))
        })?
    };
    let set = unsafe {
        dev.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: 1,
            p_set_layouts: &layout,
            ..Default::default()
        })
        .map_err(|e| {
            dev.destroy_descriptor_pool(pool, None);
            dev.destroy_descriptor_set_layout(layout, None);
            zengpu_hal::GpuError::Backend(format!("allocate_descriptor_sets: {e}"))
        })?[0]
    };
    Ok((pool, layout, set))
}

fn fill_bindless_slots(
    dev: &ash::Device,
    set: vk::DescriptorSet,
    view: vk::ImageView,
    sampler: vk::Sampler,
) {
    let image_infos: Vec<vk::DescriptorImageInfo> = (0..BINDLESS)
        .map(|_| vk::DescriptorImageInfo {
            sampler,
            image_view: view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        })
        .collect();
    unsafe {
        dev.update_descriptor_sets(
            &[vk::WriteDescriptorSet {
                dst_set: set,
                dst_binding: 0,
                dst_array_element: 0,
                descriptor_count: BINDLESS,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: image_infos.as_ptr(),
                ..Default::default()
            }],
            &[],
        )
    };
}

fn update_bindless_slot(
    dev: &ash::Device,
    set: vk::DescriptorSet,
    slot: u32,
    view: vk::ImageView,
    sampler: vk::Sampler,
) {
    let image_info = vk::DescriptorImageInfo {
        sampler,
        image_view: view,
        image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    };
    unsafe {
        dev.update_descriptor_sets(
            &[vk::WriteDescriptorSet {
                dst_set: set,
                dst_binding: 0,
                dst_array_element: slot,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: &image_info,
                ..Default::default()
            }],
            &[],
        )
    };
}

fn make_shader(dev: &ash::Device, spv: &[u32]) -> Result<vk::ShaderModule> {
    unsafe {
        dev.create_shader_module(
            &vk::ShaderModuleCreateInfo {
                code_size: spv.len() * 4,
                p_code: spv.as_ptr(),
                ..Default::default()
            },
            None,
        )
        .map_err(|e| zengpu_hal::GpuError::Backend(format!("create_shader_module: {e}")))
    }
}

fn make_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let vert = make_shader(dev, VERT_SPV)?;
    let frag = make_shader(dev, FRAG_SPV)?;
    let entry = std::ffi::CString::new("main").unwrap();
    let stages = [
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::VERTEX,
            module: vert,
            p_name: entry.as_ptr(),
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::FRAGMENT,
            module: frag,
            p_name: entry.as_ptr(),
            ..Default::default()
        },
    ];
    let push_range = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::FRAGMENT,
        offset: 0,
        size: 4,
    };
    let layout = unsafe {
        dev.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo {
                set_layout_count: 1,
                p_set_layouts: &set_layout,
                push_constant_range_count: 1,
                p_push_constant_ranges: &push_range,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| zengpu_hal::GpuError::Backend(format!("create_pipeline_layout: {e}")))?
    };
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let blend_att = vk::PipelineColorBlendAttachmentState {
        color_write_mask: vk::ColorComponentFlags::RGBA,
        ..Default::default()
    };
    let pipeline = unsafe {
        dev.create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[vk::GraphicsPipelineCreateInfo {
                stage_count: stages.len() as u32,
                p_stages: stages.as_ptr(),
                p_vertex_input_state: &vk::PipelineVertexInputStateCreateInfo::default(),
                p_input_assembly_state: &vk::PipelineInputAssemblyStateCreateInfo {
                    topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                    ..Default::default()
                },
                p_viewport_state: &vk::PipelineViewportStateCreateInfo {
                    viewport_count: 1,
                    scissor_count: 1,
                    ..Default::default()
                },
                p_rasterization_state: &vk::PipelineRasterizationStateCreateInfo {
                    polygon_mode: vk::PolygonMode::FILL,
                    cull_mode: vk::CullModeFlags::NONE,
                    front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                    line_width: 1.0,
                    ..Default::default()
                },
                p_multisample_state: &vk::PipelineMultisampleStateCreateInfo {
                    rasterization_samples: vk::SampleCountFlags::TYPE_1,
                    ..Default::default()
                },
                p_color_blend_state: &vk::PipelineColorBlendStateCreateInfo {
                    attachment_count: 1,
                    p_attachments: &blend_att,
                    ..Default::default()
                },
                p_dynamic_state: &vk::PipelineDynamicStateCreateInfo {
                    dynamic_state_count: dynamic_states.len() as u32,
                    p_dynamic_states: dynamic_states.as_ptr(),
                    ..Default::default()
                },
                layout,
                render_pass,
                subpass: 0,
                ..Default::default()
            }],
            None,
        )
        .map_err(|(_, e)| zengpu_hal::GpuError::Backend(format!("create_graphics_pipelines: {e}")))?
        .into_iter()
        .next()
        .unwrap()
    };
    unsafe {
        dev.destroy_shader_module(vert, None);
        dev.destroy_shader_module(frag, None);
    }
    Ok((layout, pipeline))
}

// ── Checkerboard texture data ─────────────────────────────────────────────────

fn checkerboard() -> Vec<u8> {
    let mut pixels = vec![0u8; (TEX_SIZE * TEX_SIZE * 4) as usize];
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let checker = ((x / CELL) + (y / CELL)) % 2 == 0;
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

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — Textured Quad (bindless slot 0)", W as i32, H as i32)?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst.request_vulkan_adapter().ok_or("no Vulkan adapter found")?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let tex: TextureHandle = device.create_texture(TextureDesc {
        width: TEX_SIZE,
        height: TEX_SIZE,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        samples: 1,
    })?;
    device.upload_texture_data(tex, &checkerboard())?;

    let samp = device.create_sampler(SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Nearest,
        ..SamplerDesc::default()
    })?;

    let tex_view = device.texture_view(tex).ok_or("stale texture handle")?;
    let samp_vk = device.sampler_vk(samp).ok_or("stale sampler handle")?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle: {e:?}"))?;
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: W,
        height: H,
        present_mode: PresentMode::Fifo,
    };

    let mut surface = TexturedSurface::new(&device, &handles, config, tex_view, samp_vk)?;

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
        surface.present()?;
    }

    drop(surface);
    device.destroy_sampler(samp);
    device.destroy_texture(tex);

    Ok(())
}
