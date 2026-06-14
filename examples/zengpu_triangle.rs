//! ZenGPU triangle rendered into an aurea window — user-side surface.
//!
//! Triangle pipeline lives here; `zengpu-vulkan` owns only swapchain + sync.
//!
//! Run:  cargo run --example zengpu_triangle

use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu_hal::{
    DeviceRequest, Format, GpuAdapter, GpuError, PresentMode, Result, SurfaceConfig, WindowHandles,
};
use zengpu_vulkan::{ash, vk, BeginFrame, DeviceContext, Swapchain, VulkanDevice, VulkanInstance};

const MAX_FRAMES: usize = 2;

const VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    void main() {
        vec2 pos[3] = vec2[](vec2(-0.5, 0.5), vec2(0.5, 0.5), vec2(0.0, -0.5));
        gl_Position = vec4(pos[gl_VertexIndex], 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(0.1, 0.6, 1.0, 1.0); }
    "#,
    frag,
    vulkan1_0
);

// ── TriangleSurface ───────────────────────────────────────────────────────────

struct TriangleSurface {
    ctx: DeviceContext,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sc: Swapchain,
}

impl TriangleSurface {
    fn new(device: &VulkanDevice, handles: &WindowHandles, config: SurfaceConfig) -> Result<Self> {
        let sc = Swapchain::new(device, handles, config, MAX_FRAMES)?;
        let ctx = sc.context();
        let dev = ctx.device();
        let render_pass = make_render_pass(dev, sc.format())?;
        let framebuffers = make_framebuffers(dev, render_pass, &sc.image_views(), sc.extent())?;
        let (pipeline_layout, pipeline) = make_pipeline(dev, render_pass)?;
        Ok(Self { ctx, render_pass, framebuffers, pipeline_layout, pipeline, sc })
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
                .map_err(|e| GpuError::Backend(format!("reset_command_buffer: {e}")))?;
            dev.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| GpuError::Backend(format!("begin_command_buffer: {e}")))?;

            let clear =
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.02, 0.02, 0.02, 1.0] } };
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
            dev.cmd_draw(cmd, 3, 1, 0, 0);
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

impl Drop for TriangleSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ctx.device().device_wait_idle();
            let dev = self.ctx.device();
            dev.destroy_pipeline(self.pipeline, None);
            dev.destroy_pipeline_layout(self.pipeline_layout, None);
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
        .map_err(|e| GpuError::Backend(format!("create_render_pass: {e}")))
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
                .map_err(|e| GpuError::Backend(format!("create_framebuffer: {e}")))
            }
        })
        .collect()
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

fn make_pipeline(
    dev: &ash::Device,
    render_pass: vk::RenderPass,
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
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let blend_att = vk::PipelineColorBlendAttachmentState {
        color_write_mask: vk::ColorComponentFlags::RGBA,
        ..Default::default()
    };
    let layout = unsafe {
        dev.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default(), None)
            .map_err(|e| GpuError::Backend(format!("create_pipeline_layout: {e}")))?
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
    Ok((layout, pipeline))
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — Triangle", 800, 600)?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst.request_vulkan_adapter().ok_or("no Vulkan adapter found")?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle: {e:?}"))?;
    let (w, h) = window.size();
    let config = SurfaceConfig { format: Format::Bgra8Unorm, width: w, height: h, present_mode: PresentMode::Fifo };

    let mut surface = TriangleSurface::new(&device, &handles, config)?;

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

    Ok(())
}
