use aurea::{Window, WindowEvent};
use inline_spirv::inline_spirv;
use zengpu::vulkan::{ash, to_vk_format, vk};
use zengpu::{
    AttachmentUsage, BeginFrame, DEPTH_FORMAT, DepthTarget, DeviceContext, DeviceRequest,
    Format, FrameGraph, GpuAdapter, GpuError, PresentMode, Result, SurfaceConfig, Swapchain,
    VulkanDevice, VulkanInstance, WindowHandles,
};

// ── Geometry ──────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
struct Vertex3d {
    pos: [f32; 3],
    color: [f32; 3],
}

fn cube_vertices() -> [Vertex3d; 8] {
    let v = |x: f32, y: f32, z: f32| Vertex3d {
        pos: [x, y, z],
        color: [x * 0.5 + 0.5, y * 0.5 + 0.5, z * 0.5 + 0.5],
    };
    [
        v(-1.0, -1.0, -1.0), v( 1.0, -1.0, -1.0),
        v( 1.0,  1.0, -1.0), v(-1.0,  1.0, -1.0),
        v(-1.0, -1.0,  1.0), v( 1.0, -1.0,  1.0),
        v( 1.0,  1.0,  1.0), v(-1.0,  1.0,  1.0),
    ]
}

#[rustfmt::skip]
const CUBE_IDX: [u32; 36] = [
    4,5,6, 4,6,7,  1,0,3, 1,3,2,
    0,4,7, 0,7,3,  5,1,2, 5,2,6,
    3,7,6, 3,6,2,  0,1,5, 0,5,4,
];

// ── Mat4 helpers ──────────────────────────────────────────────────────────────

type Mat4 = [f32; 16];

fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0f32; 16];
    for c in 0..4 { for r in 0..4 {
        o[c*4+r] = (0..4).map(|k| a[k*4+r] * b[c*4+k]).sum();
    }}
    o
}

fn identity() -> Mat4 {
    let mut m = [0.0f32; 16];
    m[0]=1.; m[5]=1.; m[10]=1.; m[15]=1.; m
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity(); m[12]=x; m[13]=y; m[14]=z; m
}

fn rotate_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity(); m[0]=c; m[8]=s; m[2]=-s; m[10]=c; m
}

fn rotate_x(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity(); m[5]=c; m[9]=-s; m[6]=s; m[10]=c; m
}

fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = f / aspect; m[5] = f;
    m[10] = far / (near - far); m[11] = -1.0;
    m[14] = (far * near) / (near - far); m
}

// ── Shaders ───────────────────────────────────────────────────────────────────

const VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec3 in_pos;
    layout(location = 1) in vec3 in_color;
    layout(push_constant) uniform PC { mat4 mvp; } pc;
    layout(location = 0) out vec3 v_color;
    void main() {
        gl_Position = pc.mvp * vec4(in_pos, 1.0);
        v_color = in_color;
    }
    "#,
    vert, vulkan1_0
);

const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec3 v_color;
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(v_color, 1.0); }
    "#,
    frag, vulkan1_0
);

// ── Surface ───────────────────────────────────────────────────────────────────

fn as_bytes<T>(s: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

/// 3D cube surface using [`FrameGraph`] for automatic barrier management.
///
/// Frame graph per frame:
///   resources: swapchain image (color), depth target (depth)
///   pass:      ColorWrite(sc) + DepthWrite(depth) → cube draw
///   present:   mark_present(sc) → frame-graph adds COLOR_ATTACHMENT→PRESENT barrier
struct CubeFgSurface {
    ctx: DeviceContext,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vertex_buf: vk::Buffer,
    vertex_mem: vk::DeviceMemory,
    index_buf: vk::Buffer,
    index_mem: vk::DeviceMemory,
    depth: DepthTarget,
    sc: Swapchain,
}

impl CubeFgSurface {
    fn new(
        device: &VulkanDevice,
        handles: &WindowHandles,
        config: SurfaceConfig,
    ) -> Result<Self> {
        let sc = Swapchain::new(device, handles, config, 2)?;
        let ctx = sc.context();
        let dev = ctx.device();

        let render_pass = make_render_pass(dev, to_vk_format(sc.format()))?;
        let (sw, sh) = sc.extent();
        let depth = DepthTarget::new(&ctx, sw, sh)?;
        let framebuffers = make_framebuffers(
            dev, render_pass, &sc.image_views(), depth.view(), vk::Extent2D { width: sw, height: sh },
        )?;
        let (pipeline_layout, pipeline) = make_pipeline(dev, render_pass)?;

        let verts = cube_vertices();
        let (vertex_buf, vertex_mem) =
            make_host_buffer(&ctx, as_bytes(&verts), vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let (index_buf, index_mem) =
            make_host_buffer(&ctx, as_bytes(&CUBE_IDX), vk::BufferUsageFlags::INDEX_BUFFER)?;

        Ok(Self {
            ctx, render_pass, framebuffers, pipeline_layout, pipeline,
            vertex_buf, vertex_mem, index_buf, index_mem, depth, sc,
        })
    }

    fn rebuild(&mut self) -> Result<()> {
        let dev = self.ctx.device();
        unsafe { let _ = dev.device_wait_idle(); }
        for &fb in &self.framebuffers {
            unsafe { dev.destroy_framebuffer(fb, None); }
        }
        let (sw, sh) = self.sc.extent();
        // Rebuild depth target at new size.
        let new_depth = DepthTarget::new(&self.ctx, sw, sh)?;
        self.framebuffers = make_framebuffers(
            dev, self.render_pass, &self.sc.image_views(), new_depth.view(),
            vk::Extent2D { width: sw, height: sh },
        )?;
        self.depth = new_depth;
        Ok(())
    }

    fn present(&mut self, mvp: &Mat4) -> Result<()> {
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

        // Build frame graph.
        let mut graph = FrameGraph::new();

        let sc_id = graph.add_swapchain_image(&self.sc, index);
        let depth_id = graph.add_depth(&self.depth);

        // ── Pass: cube draw ───────────────────────────────────────────────
        {
            let ctx = self.ctx.clone();
            let rp = self.render_pass;
            let fb = self.framebuffers[index as usize];
            let pl = self.pipeline;
            let pll = self.pipeline_layout;
            let vb = self.vertex_buf;
            let ib = self.index_buf;
            let (sw, sh) = self.sc.extent();
            let ext = vk::Extent2D { width: sw, height: sh };
            let mvp = *mvp;

            graph.add_pass(
                &[
                    (sc_id, AttachmentUsage::ColorWrite),
                    (depth_id, AttachmentUsage::DepthWrite),
                ],
                move |cmd| {
                    let dev = ctx.device();
                    let clears = [
                        vk::ClearValue {
                            color: vk::ClearColorValue { float32: [0.02, 0.02, 0.05, 1.0] },
                        },
                        vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                        },
                    ];
                    unsafe {
                        dev.cmd_begin_render_pass(
                            cmd,
                            &vk::RenderPassBeginInfo {
                                render_pass: rp,
                                framebuffer: fb,
                                render_area: vk::Rect2D {
                                    offset: vk::Offset2D::default(),
                                    extent: ext,
                                },
                                clear_value_count: 2,
                                p_clear_values: clears.as_ptr(),
                                ..Default::default()
                            },
                            vk::SubpassContents::INLINE,
                        );
                        dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pl);
                        // Negative height flips Vulkan Y without a Y-flip matrix.
                        dev.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                            x: 0.0,
                            y: ext.height as f32,
                            width: ext.width as f32,
                            height: -(ext.height as f32),
                            min_depth: 0.0,
                            max_depth: 1.0,
                        }]);
                        dev.cmd_set_scissor(
                            cmd, 0,
                            &[vk::Rect2D { offset: vk::Offset2D::default(), extent: ext }],
                        );
                        dev.cmd_push_constants(
                            cmd, pll, vk::ShaderStageFlags::VERTEX, 0,
                            as_bytes(std::slice::from_ref(&mvp)),
                        );
                        dev.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
                        dev.cmd_bind_index_buffer(cmd, ib, 0, vk::IndexType::UINT32);
                        dev.cmd_draw_indexed(cmd, CUBE_IDX.len() as u32, 1, 0, 0, 0);
                        dev.cmd_end_render_pass(cmd);
                    }
                    Ok(())
                },
            );
        }

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

    fn aspect(&self) -> f32 {
        let (sw, sh) = self.sc.extent();
        sw as f32 / sh.max(1) as f32
    }
}

impl Drop for CubeFgSurface {
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
            dev.destroy_buffer(self.index_buf, None);
            dev.free_memory(self.index_mem, None);
            dev.destroy_buffer(self.vertex_buf, None);
            dev.free_memory(self.vertex_mem, None);
            // self.depth drops here, then self.sc drops last
        }
    }
}

// ── Vulkan helpers ─────────────────────────────────────────────────────────────

fn make_render_pass(dev: &ash::Device, color_fmt: vk::Format) -> Result<vk::RenderPass> {
    let atts = [
        // Color: finalLayout = COLOR_ATTACHMENT_OPTIMAL (frame-graph handles present transition)
        vk::AttachmentDescription {
            format: color_fmt,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ..Default::default()
        },
        // Depth
        vk::AttachmentDescription {
            format: DEPTH_FORMAT,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::DONT_CARE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ..Default::default()
        },
    ];
    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let subpass = vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
        color_attachment_count: 1,
        p_color_attachments: &color_ref,
        p_depth_stencil_attachment: &depth_ref,
        ..Default::default()
    };
    let dep = vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ..Default::default()
    };
    unsafe {
        dev.create_render_pass(
            &vk::RenderPassCreateInfo {
                attachment_count: atts.len() as u32,
                p_attachments: atts.as_ptr(),
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
    color_views: &[vk::ImageView],
    depth_view: vk::ImageView,
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    color_views
        .iter()
        .map(|&cv| {
            let views = [cv, depth_view];
            unsafe {
                dev.create_framebuffer(
                    &vk::FramebufferCreateInfo {
                        render_pass,
                        attachment_count: 2,
                        p_attachments: views.as_ptr(),
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

fn make_host_buffer(
    ctx: &DeviceContext,
    data: &[u8],
    usage: vk::BufferUsageFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let dev = ctx.device();
    let size = data.len() as u64;
    let buf = unsafe {
        dev.create_buffer(
            &vk::BufferCreateInfo {
                size,
                usage,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("create_buffer: {e}")))?
    };
    let reqs = unsafe { dev.get_buffer_memory_requirements(buf) };
    let props = ctx.memory_properties();
    let mem_type = (0..props.memory_type_count)
        .find(|&i| {
            (reqs.memory_type_bits & (1 << i)) != 0
                && props.memory_types[i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)
        })
        .ok_or_else(|| GpuError::Backend("no host-visible memory".to_string()))?;
    let mem = unsafe {
        dev.allocate_memory(
            &vk::MemoryAllocateInfo {
                allocation_size: reqs.size,
                memory_type_index: mem_type,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| GpuError::Backend(format!("allocate_memory: {e}")))?
    };
    unsafe {
        dev.bind_buffer_memory(buf, mem, 0)
            .map_err(|e| GpuError::Backend(format!("bind_buffer_memory: {e}")))?;
        let ptr = dev
            .map_memory(mem, 0, size, vk::MemoryMapFlags::empty())
            .map_err(|e| GpuError::Backend(format!("map_memory: {e}")))?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        dev.unmap_memory(mem);
    }
    Ok((buf, mem))
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
    let pc = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: 64, // mat4
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
    let bindings = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: std::mem::size_of::<Vertex3d>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];
    let attrs = [
        vk::VertexInputAttributeDescription {
            location: 0, binding: 0,
            format: vk::Format::R32G32B32_SFLOAT, offset: 0,
        },
        vk::VertexInputAttributeDescription {
            location: 1, binding: 0,
            format: vk::Format::R32G32B32_SFLOAT,
            offset: std::mem::size_of::<[f32; 3]>() as u32,
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
                p_vertex_input_state: &vk::PipelineVertexInputStateCreateInfo {
                    vertex_binding_description_count: 1,
                    p_vertex_binding_descriptions: bindings.as_ptr(),
                    vertex_attribute_description_count: 2,
                    p_vertex_attribute_descriptions: attrs.as_ptr(),
                    ..Default::default()
                },
                p_input_assembly_state: &vk::PipelineInputAssemblyStateCreateInfo {
                    topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                    ..Default::default()
                },
                p_viewport_state: &vk::PipelineViewportStateCreateInfo {
                    viewport_count: 1, scissor_count: 1, ..Default::default()
                },
                p_rasterization_state: &vk::PipelineRasterizationStateCreateInfo {
                    polygon_mode: vk::PolygonMode::FILL,
                    cull_mode: vk::CullModeFlags::BACK,
                    front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                    line_width: 1.0,
                    ..Default::default()
                },
                p_multisample_state: &vk::PipelineMultisampleStateCreateInfo {
                    rasterization_samples: vk::SampleCountFlags::TYPE_1,
                    ..Default::default()
                },
                p_depth_stencil_state: &vk::PipelineDepthStencilStateCreateInfo {
                    depth_test_enable: vk::TRUE,
                    depth_write_enable: vk::TRUE,
                    depth_compare_op: vk::CompareOp::LESS,
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
    Ok((layout, pipeline))
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let window = Window::new("ZenGPU — Cube via FrameGraph (G7)", 800, 600)
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

    let mut surface = CubeFgSurface::new(&device, &handles, config)?;
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

        let t = start.elapsed().as_secs_f32();
        let model = mat_mul(&rotate_x(t * 0.4), &rotate_y(t * 0.7));
        let view = translate(0.0, 0.0, -4.0);
        let proj = perspective(
            std::f32::consts::FRAC_PI_4,
            surface.aspect(),
            0.1,
            100.0,
        );
        let mvp = mat_mul(&proj, &mat_mul(&view, &model));
        surface.present(&mvp)?;
    }
    Ok(())
}
