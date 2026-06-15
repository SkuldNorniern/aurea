use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu_hal::{
    DeviceRequest, Format, GpuAdapter, GpuError, PresentMode, Result, SurfaceConfig, WindowHandles,
};
use zengpu_vulkan::{
    ash, vk, AttachmentUsage, BeginFrame, DeviceContext, FrameGraph, OffscreenTarget, Swapchain,
    VulkanDevice, VulkanInstance,
};

const OFF_W: u32 = 512;
const OFF_H: u32 = 512;
const OFF_FMT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const MAX_FRAMES: usize = 2;

// ── Shaders ───────────────────────────────────────────────────────────────────

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

const SCR_VERT: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) out vec2 v_uv;
    void main() {
        const vec2 pos[6] = vec2[](
            vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0, 1.0),
            vec2(-1.0, 1.0), vec2(1.0,-1.0), vec2(1.0,  1.0)
        );
        const vec2 uv[6] = vec2[](
            vec2(0.0,0.0), vec2(1.0,0.0), vec2(0.0,1.0),
            vec2(0.0,1.0), vec2(1.0,0.0), vec2(1.0,1.0)
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

/// Two-pass frame-graph surface:
///   Pass 0 — "offscreen": renders spinning triangle into OffscreenTarget.
///   Pass 1 — "screen":    samples offscreen target, blits to swapchain image.
///
/// Barriers between passes are injected automatically by [`FrameGraph`] based
/// on the declared resource usages. No manual `cmd_pipeline_barrier` in user code.
struct FgSurface {
    ctx: DeviceContext,
    // offscreen resources
    offscreen: OffscreenTarget,
    off_render_pass: vk::RenderPass,  // finalLayout = COLOR_ATTACHMENT_OPTIMAL
    off_framebuffer: vk::Framebuffer,
    off_pipeline_layout: vk::PipelineLayout,
    off_pipeline: vk::Pipeline,
    // screen resources
    scr_render_pass: vk::RenderPass,  // finalLayout = COLOR_ATTACHMENT_OPTIMAL
    scr_framebuffers: Vec<vk::Framebuffer>,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,
    scr_pipeline_layout: vk::PipelineLayout,
    scr_pipeline: vk::Pipeline,
    sc: Swapchain,
}

impl FgSurface {
    fn new(device: &VulkanDevice, handles: &WindowHandles, config: SurfaceConfig) -> Result<Self> {
        let sc = Swapchain::new(device, handles, config, MAX_FRAMES)?;
        let ctx = sc.context();
        let dev = ctx.device();

        let offscreen = OffscreenTarget::new(&ctx, OFF_FMT, OFF_W, OFF_H)?;

        // Render passes: finalLayout = COLOR_ATTACHMENT_OPTIMAL.
        // The frame-graph handles the COLOR_ATTACHMENT_OPTIMAL→PRESENT_SRC_KHR
        // barrier for the swapchain image, and the COLOR_ATTACHMENT_OPTIMAL→
        // SHADER_READ_ONLY_OPTIMAL barrier for the offscreen image.
        let off_render_pass = make_render_pass(dev, OFF_FMT)?;
        let off_framebuffer = make_framebuffer(dev, off_render_pass, offscreen.view(),
                                               offscreen.extent())?;
        let (off_pipeline_layout, off_pipeline) = make_off_pipeline(dev, off_render_pass)?;

        let scr_render_pass = make_render_pass(dev, sc.format())?;
        let scr_framebuffers =
            make_framebuffers(dev, scr_render_pass, &sc.image_views(), sc.extent())?;

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
        self.scr_framebuffers = make_framebuffers(
            dev,
            self.scr_render_pass,
            &self.sc.image_views(),
            self.sc.extent(),
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
        unsafe {
            dev.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .map_err(|e| GpuError::Backend(format!("reset_command_buffer: {e}")))?;
            dev.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| GpuError::Backend(format!("begin_command_buffer: {e}")))?;
        }

        // Build the frame graph for this frame.
        let mut graph = FrameGraph::new();

        let off_id = graph.add_resource(
            self.offscreen.image(),
            self.offscreen.view(),
            self.offscreen.format(),
            self.offscreen.extent(),
            vk::ImageLayout::UNDEFINED,
        );

        let sc_images = self.sc.images();
        let sc_views = self.sc.image_views();
        let sc_id = graph.add_resource(
            sc_images[index as usize],
            sc_views[index as usize],
            self.sc.format(),
            self.sc.extent(),
            vk::ImageLayout::UNDEFINED,
        );

        // ── Pass 0: offscreen triangle ────────────────────────────────────
        {
            let ctx = self.ctx.clone();
            let off_rp = self.off_render_pass;
            let off_fb = self.off_framebuffer;
            let off_pl = self.off_pipeline;
            let off_pll = self.off_pipeline_layout;
            let off_ext = self.offscreen.extent();

            graph.add_pass(&[(off_id, AttachmentUsage::ColorWrite)], move |cmd| {
                let dev = ctx.device();
                let clear = vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.05, 0.05, 0.12, 1.0] },
                };
                unsafe {
                    dev.cmd_begin_render_pass(
                        cmd,
                        &vk::RenderPassBeginInfo {
                            render_pass: off_rp,
                            framebuffer: off_fb,
                            render_area: vk::Rect2D {
                                offset: vk::Offset2D::default(),
                                extent: off_ext,
                            },
                            clear_value_count: 1,
                            p_clear_values: &clear,
                            ..Default::default()
                        },
                        vk::SubpassContents::INLINE,
                    );
                    dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, off_pl);
                    dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                        x: 0.0, y: 0.0,
                        width: off_ext.width as f32,
                        height: off_ext.height as f32,
                        min_depth: 0.0, max_depth: 1.0,
                    }]);
                    dev.cmd_set_scissor(
                        cmd, 0,
                        &[vk::Rect2D { offset: vk::Offset2D::default(), extent: off_ext }],
                    );
                    dev.cmd_push_constants(
                        cmd, off_pll, vk::ShaderStageFlags::VERTEX, 0,
                        &angle.to_ne_bytes(),
                    );
                    dev.cmd_draw(cmd, 3, 1, 0, 0);
                    dev.cmd_end_render_pass(cmd);
                }
                Ok(())
            });
        }

        // ── Pass 1: screen quad ───────────────────────────────────────────
        {
            let ctx = self.ctx.clone();
            let scr_rp = self.scr_render_pass;
            let scr_fb = self.scr_framebuffers[index as usize];
            let scr_pl = self.scr_pipeline;
            let scr_pll = self.scr_pipeline_layout;
            let ds = self.descriptor_set;
            let sc_ext = self.sc.extent();

            graph.add_pass(
                &[
                    (off_id, AttachmentUsage::ShaderSample),
                    (sc_id, AttachmentUsage::ColorWrite),
                ],
                move |cmd| {
                    let dev = ctx.device();
                    let clear = vk::ClearValue {
                        color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] },
                    };
                    unsafe {
                        dev.cmd_begin_render_pass(
                            cmd,
                            &vk::RenderPassBeginInfo {
                                render_pass: scr_rp,
                                framebuffer: scr_fb,
                                render_area: vk::Rect2D {
                                    offset: vk::Offset2D::default(),
                                    extent: sc_ext,
                                },
                                clear_value_count: 1,
                                p_clear_values: &clear,
                                ..Default::default()
                            },
                            vk::SubpassContents::INLINE,
                        );
                        dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, scr_pl);
                        dev.cmd_bind_descriptor_sets(
                            cmd, vk::PipelineBindPoint::GRAPHICS, scr_pll,
                            0, &[ds], &[],
                        );
                        dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                            x: 0.0, y: 0.0,
                            width: sc_ext.width as f32,
                            height: sc_ext.height as f32,
                            min_depth: 0.0, max_depth: 1.0,
                        }]);
                        dev.cmd_set_scissor(
                            cmd, 0,
                            &[vk::Rect2D { offset: vk::Offset2D::default(), extent: sc_ext }],
                        );
                        dev.cmd_draw(cmd, 6, 1, 0, 0);
                        dev.cmd_end_render_pass(cmd);
                    }
                    Ok(())
                },
            );
        }

        // Frame-graph owns the present barrier — no manual barrier in user code.
        graph.mark_present(sc_id);
        graph.execute(cmd, &self.ctx)?;

        unsafe {
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

impl Drop for FgSurface {
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
        }
    }
}

// ── Vulkan helpers ─────────────────────────────────────────────────────────────

/// Generic render pass: CLEAR→STORE, finalLayout = COLOR_ATTACHMENT_OPTIMAL.
/// The frame-graph handles all transitions into/out of COLOR_ATTACHMENT_OPTIMAL.
fn make_render_pass(dev: &ash::Device, format: vk::Format) -> Result<vk::RenderPass> {
    let att = vk::AttachmentDescription {
        format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
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
                p_attachments: &att,
                subpass_count: 1,
                p_subpasses: &subpass,
                dependency_count: 1,
                p_dependencies: &dep,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create_render_pass: {e}")))
    }
}

fn make_framebuffer(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    view: vk::ImageView,
    extent: vk::Extent2D,
) -> Result<vk::Framebuffer> {
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
        .map_err(|e| GpuError::Backend(format!("create_framebuffer: {e}")))
    }
}

fn make_framebuffers(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
    views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    views.iter().map(|&v| make_framebuffer(dev, render_pass, v, extent)).collect()
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
        .map_err(|e| GpuError::Backend(format!("create_sampler: {e}")))
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
        .map_err(|e| GpuError::Backend(format!("create_descriptor_pool: {e}")))?
    };
    let binding = vk::DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 1,
        stage_flags: vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let set_layout = unsafe {
        dev.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo {
                binding_count: 1,
                p_bindings: &binding,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create_descriptor_set_layout: {e}")))?
    };
    let set = unsafe {
        dev.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: 1,
            p_set_layouts: &set_layout,
            ..Default::default()
        })
        .map_err(|e| GpuError::Backend(format!("allocate_descriptor_sets: {e}")))?[0]
    };
    let img_info = vk::DescriptorImageInfo {
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
                p_image_info: &img_info,
                ..Default::default()
            }],
            &[],
        );
    }
    Ok((pool, set_layout, set))
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
                stage_count: 2,
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
                    dynamic_state_count: 2,
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

fn make_off_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let pc = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: 4,
    };
    let layout = unsafe {
        dev.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo {
                push_constant_range_count: 1,
                p_push_constant_ranges: &pc,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create_pipeline_layout: {e}")))?
    };
    Ok((layout, build_pipeline(dev, render_pass, layout, OFF_VERT, OFF_FRAG)?))
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
        .map_err(|e| GpuError::Backend(format!("create_pipeline_layout: {e}")))?
    };
    Ok((layout, build_pipeline(dev, render_pass, layout, SCR_VERT, SCR_FRAG)?))
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — Frame Graph (G6)", 800, 600)?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst.request_vulkan_adapter().ok_or("no Vulkan adapter")?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle: {e:?}"))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };

    let mut surface = FgSurface::new(&device, &handles, config)?;

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
        surface.present(start.elapsed().as_secs_f32())?;
    }
    Ok(())
}
