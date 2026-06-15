use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu::vulkan::{ash, to_vk_format, vk};
use zengpu::{
    BeginFrame, DeviceContext, DeviceRequest, Format, GpuAdapter, GpuError, OffscreenTarget,
    PresentMode, Result, SurfaceConfig, Swapchain, VulkanDevice, VulkanInstance, WindowHandles,
};

// Offscreen render size (fixed; independent of window size).
const OFF_W: u32 = 512;
const OFF_H: u32 = 512;
const OFF_FMT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const MAX_FRAMES: usize = 2;

// ── Offscreen shaders: spinning triangle ─────────────────────────────────────

const OFF_VERT: &[u32] = inline_spirv!(
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

const OFF_FRAG: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec3 v_color;
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(v_color, 1.0); }
    "#,
    frag,
    vulkan1_0
);

// ── Screen shaders: fullscreen quad sampling offscreen texture ────────────────

const SCR_VERT: &[u32] = inline_spirv!(
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

const SCR_FRAG: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec2 v_uv;
    layout(location = 0) out vec4 o_color;
    layout(set = 0, binding = 0) uniform sampler2D u_tex;
    void main() { o_color = texture(u_tex, v_uv); }
    "#,
    frag,
    vulkan1_0
);

// ── Surface ───────────────────────────────────────────────────────────────────

/// Owns both the offscreen pass (triangle → texture) and the screen pass
/// (texture → swapchain). Records both into one command buffer per frame.
struct OffscreenSurface {
    ctx: DeviceContext,
    // offscreen resources
    offscreen: OffscreenTarget,
    off_render_pass: vk::RenderPass,
    off_framebuffer: vk::Framebuffer,
    off_pipeline_layout: vk::PipelineLayout,
    off_pipeline: vk::Pipeline,
    // screen resources
    scr_render_pass: vk::RenderPass,
    scr_framebuffers: Vec<vk::Framebuffer>,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,
    scr_pipeline_layout: vk::PipelineLayout,
    scr_pipeline: vk::Pipeline,
    // swapchain last for drop order
    sc: Swapchain,
}

impl OffscreenSurface {
    fn new(device: &VulkanDevice, handles: &WindowHandles, config: SurfaceConfig) -> Result<Self> {
        let sc = Swapchain::new(device, handles, config, MAX_FRAMES)?;
        let ctx = sc.context();
        let dev = ctx.device();

        // offscreen target
        let offscreen = OffscreenTarget::new(&ctx, Format::Rgba8Unorm, OFF_W, OFF_H)?;

        // offscreen pass: UNDEFINED→CLEAR→SHADER_READ_ONLY_OPTIMAL
        let off_render_pass = make_off_render_pass(dev)?;
        let off_framebuffer = make_off_framebuffer(dev, off_render_pass, &offscreen)?;
        let (off_pipeline_layout, off_pipeline) = make_off_pipeline(dev, off_render_pass)?;

        // screen pass: UNDEFINED→CLEAR→PRESENT_SRC_KHR
        let scr_render_pass = make_scr_render_pass(dev, to_vk_format(sc.format()))?;
        let (sw, sh) = sc.extent();
        let scr_framebuffers = make_scr_framebuffers(
            dev, scr_render_pass, &sc.image_views(), vk::Extent2D { width: sw, height: sh },
        )?;

        // descriptor: combined image sampler for offscreen target
        let sampler = make_sampler(dev)?;
        let (descriptor_pool, descriptor_set_layout, descriptor_set) =
            make_descriptor(dev, offscreen.view(), sampler)?;

        let (scr_pipeline_layout, scr_pipeline) =
            make_scr_pipeline(dev, scr_render_pass, descriptor_set_layout)?;

        Ok(Self {
            ctx,
            offscreen,
            off_render_pass,
            off_framebuffer,
            off_pipeline_layout,
            off_pipeline,
            scr_render_pass,
            scr_framebuffers,
            sampler,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            scr_pipeline_layout,
            scr_pipeline,
            sc,
        })
    }

    fn rebuild(&mut self) -> Result<()> {
        let dev = self.ctx.device();
        unsafe {
            for &fb in &self.scr_framebuffers {
                dev.destroy_framebuffer(fb, None);
            }
        }
        let (sw, sh) = self.sc.extent();
        self.scr_framebuffers = make_scr_framebuffers(
            dev,
            self.scr_render_pass,
            &self.sc.image_views(),
            vk::Extent2D { width: sw, height: sh },
        )?;
        Ok(())
    }

    fn present(&mut self, angle: f32) -> Result<()> {
        let bf = self.sc.begin_frame()?;
        let (current, index) = match bf {
            BeginFrame::Image { current, index } => (current, index),
            BeginFrame::Recreated => return self.rebuild(),
            BeginFrame::Skip => return Ok(()),
        };

        let cmd = self.sc.cmd_buffer(current);
        let dev = self.ctx.device();
        let (scw, sch) = self.sc.extent();
        let sc_extent = vk::Extent2D { width: scw, height: sch };
        let (ow, oh) = self.offscreen.extent();
        let off_extent = vk::Extent2D { width: ow, height: oh };

        unsafe {
            dev.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .map_err(|e| GpuError::Backend(format!("reset_command_buffer: {e}")))?;
            dev.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| GpuError::Backend(format!("begin_command_buffer: {e}")))?;

            // ── 1. Offscreen pass ──────────────────────────────────────────
            let off_clear = vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.05, 0.05, 0.12, 1.0] },
            };
            dev.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo {
                    render_pass: self.off_render_pass,
                    framebuffer: self.off_framebuffer,
                    render_area: vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: off_extent,
                    },
                    clear_value_count: 1,
                    p_clear_values: &off_clear,
                    ..Default::default()
                },
                vk::SubpassContents::INLINE,
            );
            dev.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.off_pipeline,
            );
            dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: off_extent.width as f32,
                height: off_extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }]);
            dev.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D::default(), extent: off_extent }],
            );
            dev.cmd_push_constants(
                cmd,
                self.off_pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                &angle.to_ne_bytes(),
            );
            dev.cmd_draw(cmd, 3, 1, 0, 0);
            dev.cmd_end_render_pass(cmd);

            // ── 2. Barrier: offscreen write → screen sample ────────────────
            dev.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    old_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    image: self.offscreen.image(),
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    ..Default::default()
                }],
            );

            // ── 3. Screen pass ─────────────────────────────────────────────
            let scr_clear = vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] },
            };
            dev.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo {
                    render_pass: self.scr_render_pass,
                    framebuffer: self.scr_framebuffers[index as usize],
                    render_area: vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: sc_extent,
                    },
                    clear_value_count: 1,
                    p_clear_values: &scr_clear,
                    ..Default::default()
                },
                vk::SubpassContents::INLINE,
            );
            dev.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.scr_pipeline,
            );
            dev.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.scr_pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: sc_extent.width as f32,
                height: sc_extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }]);
            dev.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D::default(), extent: sc_extent }],
            );
            dev.cmd_draw(cmd, 6, 1, 0, 0); // two triangles = fullscreen quad
            dev.cmd_end_render_pass(cmd);

            dev.end_command_buffer(cmd)
                .map_err(|e| GpuError::Backend(format!("end_command_buffer: {e}")))?;
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

impl Drop for OffscreenSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ctx.device().device_wait_idle();
            let dev = self.ctx.device();
            dev.destroy_pipeline(self.scr_pipeline, None);
            dev.destroy_pipeline_layout(self.scr_pipeline_layout, None);
            dev.destroy_descriptor_pool(self.descriptor_pool, None);
            dev.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            dev.destroy_sampler(self.sampler, None);
            for &fb in &self.scr_framebuffers {
                dev.destroy_framebuffer(fb, None);
            }
            dev.destroy_render_pass(self.scr_render_pass, None);
            dev.destroy_pipeline(self.off_pipeline, None);
            dev.destroy_pipeline_layout(self.off_pipeline_layout, None);
            dev.destroy_framebuffer(self.off_framebuffer, None);
            dev.destroy_render_pass(self.off_render_pass, None);
            // self.offscreen drops here, then self.sc drops last
        }
    }
}

// ── Offscreen render pass helpers ─────────────────────────────────────────────

fn make_off_render_pass(dev: &ash::Device) -> Result<vk::RenderPass> {
    let attachment = vk::AttachmentDescription {
        format: OFF_FMT,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        // UNDEFINED: discard previous; loadOp=CLEAR so previous content irrelevant.
        initial_layout: vk::ImageLayout::UNDEFINED,
        // Leave in SHADER_READ_ONLY_OPTIMAL so the barrier + screen pass can sample it.
        final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ..Default::default()
    };
    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
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
        .map_err(|e| GpuError::Backend(format!("create off render_pass: {e}")))
    }
}

fn make_off_framebuffer(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    target: &OffscreenTarget,
) -> Result<vk::Framebuffer> {
    let view = target.view();
    let (ew, eh) = target.extent();
    unsafe {
        dev.create_framebuffer(
            &vk::FramebufferCreateInfo {
                render_pass,
                attachment_count: 1,
                p_attachments: &view,
                width: ew,
                height: eh,
                layers: 1,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create off framebuffer: {e}")))
    }
}

fn make_off_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let pc_range = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: 4, // one f32 (angle)
    };
    let layout = unsafe {
        dev.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo {
                push_constant_range_count: 1,
                p_push_constant_ranges: &pc_range,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create off pipeline_layout: {e}")))?
    };
    let pipeline = build_pipeline(dev, render_pass, layout, OFF_VERT, OFF_FRAG)?;
    Ok((layout, pipeline))
}

// ── Screen render pass helpers ────────────────────────────────────────────────

fn make_scr_render_pass(dev: &ash::Device, format: vk::Format) -> Result<vk::RenderPass> {
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
    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
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
        .map_err(|e| GpuError::Backend(format!("create scr render_pass: {e}")))
    }
}

fn make_scr_framebuffers(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    views
        .iter()
        .map(|&view| {
            unsafe {
                dev.create_framebuffer(
                    &vk::FramebufferCreateInfo {
                        render_pass,
                        attachment_count: 1,
                        p_attachments: &view,
                        width: extent.width,
                        height: extent.height,
                        layers: 1,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|e| GpuError::Backend(format!("create scr framebuffer: {e}")))
            }
        })
        .collect()
}

fn make_sampler(dev: &ash::Device) -> Result<vk::Sampler> {
    unsafe {
        dev.create_sampler(
            &vk::SamplerCreateInfo {
                mag_filter: vk::Filter::LINEAR,
                min_filter: vk::Filter::LINEAR,
                mipmap_mode: vk::SamplerMipmapMode::LINEAR,
                address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create sampler: {e}")))
    }
}

fn make_descriptor(
    dev: &ash::Device,
    view: vk::ImageView,
    sampler: vk::Sampler,
) -> Result<(vk::DescriptorPool, vk::DescriptorSetLayout, vk::DescriptorSet)> {
    let pool_size = vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 1,
    };
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
        .map_err(|e| GpuError::Backend(format!("create descriptor_pool: {e}")))?
    };

    let binding = vk::DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 1,
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
        .map_err(|e| GpuError::Backend(format!("create descriptor_set_layout: {e}")))?
    };

    let set = unsafe {
        dev.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: 1,
            p_set_layouts: &layout,
            ..Default::default()
        })
        .map_err(|e| GpuError::Backend(format!("allocate_descriptor_sets: {e}")))?[0]
    };

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
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: &image_info,
                ..Default::default()
            }],
            &[],
        );
    }

    Ok((pool, layout, set))
}

fn make_scr_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let layout = unsafe {
        dev.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo {
                set_layout_count: 1,
                p_set_layouts: &set_layout,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create scr pipeline_layout: {e}")))?
    };
    let pipeline = build_pipeline(dev, render_pass, layout, SCR_VERT, SCR_FRAG)?;
    Ok((layout, pipeline))
}

// ── Shared pipeline builder ───────────────────────────────────────────────────

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
        .map_err(|e| GpuError::Backend(format!("create_shader_module: {e}")))
    }
}

fn build_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    vert_spv: &[u32],
    frag_spv: &[u32],
) -> Result<vk::Pipeline> {
    let vert = make_shader(dev, vert_spv)?;
    let frag = make_shader(dev, frag_spv)?;
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
        .map_err(|(_, e)| GpuError::Backend(format!("create_graphics_pipelines: {e}")))?
        .into_iter()
        .next()
        .unwrap()
    };
    unsafe {
        dev.destroy_shader_module(vert, None);
        dev.destroy_shader_module(frag, None);
    }
    Ok(pipeline)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let window = Window::new("ZenGPU — Offscreen (render-to-texture)", 800, 600)
        .map_err(|e| GpuError::Backend(format!("window: {e}")))?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst
        .request_vulkan_adapter()
        .ok_or_else(|| GpuError::Backend("no Vulkan adapter".into()))?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| GpuError::Backend(format!("window handle: {e:?}")))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };

    let mut surface = OffscreenSurface::new(&device, &handles, config)?;

    let start = std::time::Instant::now();
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
        let angle = start.elapsed().as_secs_f32();
        surface.present(angle)?;
    }

    Ok(())
}
