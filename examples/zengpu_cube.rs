//! ZenGPU 3D cube rendered into an aurea window.
//!
//! Demonstrates user-side surface construction on the public `Swapchain`
//! scaffold.  All render-pass-shaped resources (render pass, depth targets,
//! pipeline, vertex/index buffers, per-frame recording) live here; the
//! surface/swapchain/sync plumbing is delegated to `zengpu_vulkan::Swapchain`.
//!
//! Memory-usage note: compare with the wgpu_window example to see the
//! overhead difference between the two backends.
//!
//! Run: `cargo run --example zengpu_cube`

use std::sync::Mutex;
use std::time::Instant;

use ash::vk;
use inline_spirv::inline_spirv;
use zengpu_hal::{DeviceRequest, Format, GpuAdapter, GpuError, PresentMode, Result, SurfaceConfig, WindowHandles};
use zengpu_vulkan::{BeginFrame, DeviceContext, Swapchain, VulkanDevice, VulkanInstance};
use aurea::{Window, WindowEvent};

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
        v(-1.0, -1.0, -1.0),
        v(1.0, -1.0, -1.0),
        v(1.0, 1.0, -1.0),
        v(-1.0, 1.0, -1.0),
        v(-1.0, -1.0, 1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(-1.0, 1.0, 1.0),
    ]
}

/// 36 indices, each face wound CCW as seen from outside (right-handed coords).
#[rustfmt::skip]
const CUBE_INDICES: [u32; 36] = [
    4, 5, 6,  4, 6, 7,
    1, 0, 3,  1, 3, 2,
    0, 4, 7,  0, 7, 3,
    5, 1, 2,  5, 2, 6,
    3, 7, 6,  3, 6, 2,
    0, 1, 5,  0, 5, 4,
];

// ── Column-major mat4 helpers ─────────────────────────────────────────────────

type Mat4 = [f32; 16];

fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

fn identity() -> Mat4 {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

fn rotate_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[8] = s;
    m[2] = -s;
    m[10] = c;
    m
}

fn rotate_x(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[5] = c;
    m[9] = -s;
    m[6] = s;
    m[10] = c;
    m
}

/// Standard right-handed perspective. Negative viewport height in `record()`
/// flips Y for Vulkan's +Y-down NDC, so no manual Y-flip is needed here.
fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = (far * near) / (near - far);
    m
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
    vert,
    vulkan1_0
);

const FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec3 v_color;
    layout(location = 0) out vec4 o_color;
    void main() { o_color = vec4(v_color, 1.0); }
    "#,
    frag,
    vulkan1_0
);

const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

// ── User-side surface ─────────────────────────────────────────────────────────

struct FrameTarget {
    framebuffer: vk::Framebuffer,
    depth_image: vk::Image,
    depth_view: vk::ImageView,
    depth_mem: vk::DeviceMemory,
}

struct CubeSurface {
    ctx: DeviceContext,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vertex_buf: vk::Buffer,
    vertex_mem: vk::DeviceMemory,
    index_buf: vk::Buffer,
    index_mem: vk::DeviceMemory,
    index_count: u32,
    targets: Mutex<Vec<FrameTarget>>,
    swapchain: Swapchain,
}

impl CubeSurface {
    fn new(
        device: &VulkanDevice,
        handles: &WindowHandles,
        config: SurfaceConfig,
        vertices: &[Vertex3d],
        indices: &[u32],
    ) -> Result<Self> {
        let swapchain = Swapchain::new(device, handles, config, 2)?;
        let ctx = swapchain.context();
        let render_pass = create_render_pass(&ctx, swapchain.format())?;
        let pipeline_layout = create_pipeline_layout(&ctx)?;
        let pipeline = create_pipeline(&ctx, render_pass, pipeline_layout)?;
        let (vertex_buf, vertex_mem) =
            create_host_buffer(&ctx, as_bytes(vertices), vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let (index_buf, index_mem) =
            create_host_buffer(&ctx, as_bytes(indices), vk::BufferUsageFlags::INDEX_BUFFER)?;
        let targets = build_targets(&ctx, render_pass, &swapchain)?;
        Ok(Self {
            ctx,
            render_pass,
            pipeline_layout,
            pipeline,
            vertex_buf,
            vertex_mem,
            index_buf,
            index_mem,
            index_count: indices.len() as u32,
            targets: Mutex::new(targets),
            swapchain,
        })
    }

    fn present(&self, mvp: &Mat4) -> Result<()> {
        let frame = self.swapchain.begin_frame()?;
        let (current, index) = match frame {
            BeginFrame::Image { current, index } => (current, index),
            BeginFrame::Recreated => { self.rebuild_targets()?; return Ok(()); }
            BeginFrame::Skip => return Ok(()),
        };
        let targets = self.targets.lock().unwrap();
        let cmd = self.swapchain.cmd_buffer(current);
        self.record(cmd, targets[index as usize].framebuffer, self.swapchain.extent(), mvp)?;
        drop(targets);
        if self.swapchain.end_frame(&frame, cmd)? {
            self.rebuild_targets()?;
        }
        Ok(())
    }

    fn record(
        &self,
        cmd: vk::CommandBuffer,
        framebuffer: vk::Framebuffer,
        extent: vk::Extent2D,
        mvp: &Mat4,
    ) -> Result<()> {
        let dev = self.ctx.device();
        let clears = [
            vk::ClearValue { color: vk::ClearColorValue { float32: [0.02, 0.02, 0.05, 1.0] } },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
            },
        ];
        unsafe {
            dev.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo {
                    flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                    ..Default::default()
                },
            )
            .map_err(|e| GpuError::Backend(format!("begin_command_buffer: {e}")))?;
            dev.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo {
                    render_pass: self.render_pass,
                    framebuffer,
                    render_area: vk::Rect2D { offset: vk::Offset2D::default(), extent },
                    clear_value_count: clears.len() as u32,
                    p_clear_values: clears.as_ptr(),
                    ..Default::default()
                },
                vk::SubpassContents::INLINE,
            );
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            dev.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: extent.height as f32,
                    width: extent.width as f32,
                    height: -(extent.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            dev.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D::default(), extent }],
            );
            dev.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                as_bytes(std::slice::from_ref(mvp)),
            );
            dev.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buf], &[0]);
            dev.cmd_bind_index_buffer(cmd, self.index_buf, 0, vk::IndexType::UINT32);
            dev.cmd_draw_indexed(cmd, self.index_count, 1, 0, 0, 0);
            dev.cmd_end_render_pass(cmd);
            dev.end_command_buffer(cmd)
                .map_err(|e| GpuError::Backend(format!("end_command_buffer: {e}")))?;
        }
        Ok(())
    }

    fn resize(&self, w: u32, h: u32) -> Result<()> {
        self.swapchain.resize(w, h)?;
        self.rebuild_targets()
    }

    fn rebuild_targets(&self) -> Result<()> {
        let dev = self.ctx.device();
        unsafe { let _ = dev.device_wait_idle(); }
        let mut targets = self.targets.lock().unwrap();
        for t in targets.drain(..) { destroy_target(dev, t); }
        *targets = build_targets(&self.ctx, self.render_pass, &self.swapchain)?;
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        let e = self.swapchain.extent();
        (e.width, e.height)
    }
}

impl Drop for CubeSurface {
    fn drop(&mut self) {
        let dev = self.ctx.device();
        unsafe {
            let _ = dev.device_wait_idle();
            for t in self.targets.lock().unwrap().drain(..) { destroy_target(dev, t); }
            dev.destroy_pipeline(self.pipeline, None);
            dev.destroy_pipeline_layout(self.pipeline_layout, None);
            dev.destroy_render_pass(self.render_pass, None);
            dev.destroy_buffer(self.vertex_buf, None);
            dev.free_memory(self.vertex_mem, None);
            dev.destroy_buffer(self.index_buf, None);
            dev.free_memory(self.index_mem, None);
        }
    }
}

// ── Vulkan resource construction ─────────────────────────────────────────────

fn build_targets(ctx: &DeviceContext, render_pass: vk::RenderPass, swapchain: &Swapchain) -> Result<Vec<FrameTarget>> {
    let dev = ctx.device();
    let extent = swapchain.extent();
    swapchain.image_views().into_iter().map(|color_view| {
        let (depth_image, depth_view, depth_mem) = create_depth(ctx, extent)?;
        let framebuffer = unsafe {
            dev.create_framebuffer(
                &vk::FramebufferCreateInfo {
                    render_pass,
                    attachment_count: 2,
                    p_attachments: [color_view, depth_view].as_ptr(),
                    width: extent.width,
                    height: extent.height,
                    layers: 1,
                    ..Default::default()
                },
                None,
            )
        }
        .map_err(|e| GpuError::Backend(format!("create_framebuffer: {e}")))?;
        Ok(FrameTarget { framebuffer, depth_image, depth_view, depth_mem })
    }).collect()
}

fn destroy_target(dev: &ash::Device, t: FrameTarget) {
    unsafe {
        dev.destroy_framebuffer(t.framebuffer, None);
        dev.destroy_image_view(t.depth_view, None);
        dev.destroy_image(t.depth_image, None);
        dev.free_memory(t.depth_mem, None);
    }
}

fn create_depth(ctx: &DeviceContext, extent: vk::Extent2D) -> Result<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
    let dev = ctx.device();
    let image = unsafe {
        dev.create_image(&vk::ImageCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            format: DEPTH_FORMAT,
            extent: vk::Extent3D { width: extent.width.max(1), height: extent.height.max(1), depth: 1 },
            mip_levels: 1, array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        }, None)
    }.map_err(|e| GpuError::Backend(format!("depth create_image: {e}")))?;
    let reqs = unsafe { dev.get_image_memory_requirements(image) };
    let ti = find_memory_type(ctx, reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
        .ok_or_else(|| GpuError::Backend("no device-local memory for depth".to_string()))?;
    let memory = unsafe {
        dev.allocate_memory(&vk::MemoryAllocateInfo { allocation_size: reqs.size, memory_type_index: ti, ..Default::default() }, None)
    }.map_err(|e| GpuError::Backend(format!("depth allocate_memory: {e}")))?;
    unsafe { dev.bind_image_memory(image, memory, 0) }
        .map_err(|e| GpuError::Backend(format!("depth bind_image_memory: {e}")))?;
    let view = unsafe {
        dev.create_image_view(&vk::ImageViewCreateInfo {
            image, view_type: vk::ImageViewType::TYPE_2D, format: DEPTH_FORMAT,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1,
            },
            ..Default::default()
        }, None)
    }.map_err(|e| GpuError::Backend(format!("depth create_image_view: {e}")))?;
    Ok((image, view, memory))
}

fn create_render_pass(ctx: &DeviceContext, color_format: vk::Format) -> Result<vk::RenderPass> {
    let attachments = [
        vk::AttachmentDescription {
            format: color_format, samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR, store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE, stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED, final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            ..Default::default()
        },
        vk::AttachmentDescription {
            format: DEPTH_FORMAT, samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR, store_op: vk::AttachmentStoreOp::DONT_CARE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE, stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED, final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ..Default::default()
        },
    ];
    let color_ref = vk::AttachmentReference { attachment: 0, layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL };
    let depth_ref = vk::AttachmentReference { attachment: 1, layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL };
    let subpass = vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
        color_attachment_count: 1, p_color_attachments: &color_ref,
        p_depth_stencil_attachment: &depth_ref,
        ..Default::default()
    };
    let stages = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
    let dep = vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL, dst_subpass: 0,
        src_stage_mask: stages, dst_stage_mask: stages,
        src_access_mask: vk::AccessFlags::empty(),
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ..Default::default()
    };
    unsafe {
        ctx.device().create_render_pass(&vk::RenderPassCreateInfo {
            attachment_count: attachments.len() as u32, p_attachments: attachments.as_ptr(),
            subpass_count: 1, p_subpasses: &subpass,
            dependency_count: 1, p_dependencies: &dep,
            ..Default::default()
        }, None)
    }.map_err(|e| GpuError::Backend(format!("create_render_pass: {e}")))
}

fn create_pipeline_layout(ctx: &DeviceContext) -> Result<vk::PipelineLayout> {
    let push = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX, offset: 0,
        size: std::mem::size_of::<Mat4>() as u32,
    };
    unsafe {
        ctx.device().create_pipeline_layout(&vk::PipelineLayoutCreateInfo {
            push_constant_range_count: 1, p_push_constant_ranges: &push,
            ..Default::default()
        }, None)
    }.map_err(|e| GpuError::Backend(format!("create_pipeline_layout: {e}")))
}

fn create_pipeline(ctx: &DeviceContext, render_pass: vk::RenderPass, layout: vk::PipelineLayout) -> Result<vk::Pipeline> {
    let dev = ctx.device();
    let vert = create_shader_module(dev, VERT_SPV)?;
    let frag = create_shader_module(dev, FRAG_SPV)?;
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::VERTEX, module: vert, p_name: entry.as_ptr(), ..Default::default() },
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::FRAGMENT, module: frag, p_name: entry.as_ptr(), ..Default::default() },
    ];
    let binding = vk::VertexInputBindingDescription { binding: 0, stride: std::mem::size_of::<Vertex3d>() as u32, input_rate: vk::VertexInputRate::VERTEX };
    let attrs = [
        vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32B32_SFLOAT, offset: 0 },
        vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32B32_SFLOAT, offset: 12 },
    ];
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let pipeline = unsafe {
        dev.create_graphics_pipelines(vk::PipelineCache::null(), &[vk::GraphicsPipelineCreateInfo {
            stage_count: 2, p_stages: stages.as_ptr(),
            p_vertex_input_state: &vk::PipelineVertexInputStateCreateInfo {
                vertex_binding_description_count: 1, p_vertex_binding_descriptions: &binding,
                vertex_attribute_description_count: attrs.len() as u32, p_vertex_attribute_descriptions: attrs.as_ptr(),
                ..Default::default()
            },
            p_input_assembly_state: &vk::PipelineInputAssemblyStateCreateInfo { topology: vk::PrimitiveTopology::TRIANGLE_LIST, ..Default::default() },
            p_viewport_state: &vk::PipelineViewportStateCreateInfo { viewport_count: 1, scissor_count: 1, ..Default::default() },
            p_rasterization_state: &vk::PipelineRasterizationStateCreateInfo {
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::BACK,
                front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                line_width: 1.0, ..Default::default()
            },
            p_multisample_state: &vk::PipelineMultisampleStateCreateInfo { rasterization_samples: vk::SampleCountFlags::TYPE_1, ..Default::default() },
            p_depth_stencil_state: &vk::PipelineDepthStencilStateCreateInfo {
                depth_test_enable: vk::TRUE, depth_write_enable: vk::TRUE, depth_compare_op: vk::CompareOp::LESS, ..Default::default()
            },
            p_color_blend_state: &vk::PipelineColorBlendStateCreateInfo {
                attachment_count: 1,
                p_attachments: &vk::PipelineColorBlendAttachmentState { color_write_mask: vk::ColorComponentFlags::RGBA, ..Default::default() },
                ..Default::default()
            },
            p_dynamic_state: &vk::PipelineDynamicStateCreateInfo {
                dynamic_state_count: dynamic_states.len() as u32, p_dynamic_states: dynamic_states.as_ptr(),
                ..Default::default()
            },
            layout, render_pass, subpass: 0,
            ..Default::default()
        }], None)
        .map_err(|(_, e)| GpuError::Backend(format!("create_graphics_pipelines: {e}")))?[0]
    };
    unsafe { dev.destroy_shader_module(vert, None); dev.destroy_shader_module(frag, None); }
    Ok(pipeline)
}

fn create_shader_module(dev: &ash::Device, spv: &[u32]) -> Result<vk::ShaderModule> {
    unsafe {
        dev.create_shader_module(&vk::ShaderModuleCreateInfo { code_size: spv.len() * 4, p_code: spv.as_ptr(), ..Default::default() }, None)
    }.map_err(|e| GpuError::Backend(format!("create_shader_module: {e}")))
}

fn create_host_buffer(ctx: &DeviceContext, data: &[u8], usage: vk::BufferUsageFlags) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let dev = ctx.device();
    let buffer = unsafe {
        dev.create_buffer(&vk::BufferCreateInfo { size: data.len() as u64, usage, sharing_mode: vk::SharingMode::EXCLUSIVE, ..Default::default() }, None)
    }.map_err(|e| GpuError::Backend(format!("create_buffer: {e}")))?;
    let reqs = unsafe { dev.get_buffer_memory_requirements(buffer) };
    let ti = find_memory_type(ctx, reqs.memory_type_bits, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)
        .ok_or_else(|| GpuError::Backend("no host-visible memory".to_string()))?;
    let memory = unsafe {
        dev.allocate_memory(&vk::MemoryAllocateInfo { allocation_size: reqs.size, memory_type_index: ti, ..Default::default() }, None)
    }.map_err(|e| GpuError::Backend(format!("allocate_memory: {e}")))?;
    unsafe {
        dev.bind_buffer_memory(buffer, memory, 0).map_err(|e| GpuError::Backend(format!("bind_buffer_memory: {e}")))?;
        let ptr = dev.map_memory(memory, 0, data.len() as u64, vk::MemoryMapFlags::empty())
            .map_err(|e| GpuError::Backend(format!("map_memory: {e}")))?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        dev.unmap_memory(memory);
    }
    Ok((buffer, memory))
}

fn find_memory_type(ctx: &DeviceContext, type_bits: u32, props: vk::MemoryPropertyFlags) -> Option<u32> {
    let mem = ctx.memory_properties();
    (0..mem.memory_type_count).find(|&i| {
        type_bits & (1 << i) != 0 && mem.memory_types[i as usize].property_flags.contains(props)
    })
}

fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — 3D cube", 800, 600)?;

    let instance = VulkanInstance::new_with_surface()?;
    let adapter = instance.request_vulkan_adapter().ok_or("no Vulkan adapter")?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handles: {e:?}"))?;
    let (w, h) = window.size();
    let surface = CubeSurface::new(
        &device,
        &handles,
        SurfaceConfig {
            format: Format::Bgra8Unorm,
            width: w,
            height: h,
            present_mode: PresentMode::Fifo,
        },
        &cube_vertices(),
        &CUBE_INDICES,
    )?;

    let start = Instant::now();

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

        let (w, h) = surface.size();
        let t = start.elapsed().as_secs_f32();
        let model = mat_mul(&rotate_y(t * 0.6), &rotate_x(t * 0.3));
        let view = translate(0.0, 0.0, -5.0);
        let proj = perspective(60f32.to_radians(), w as f32 / h.max(1) as f32, 0.1, 100.0);
        let mvp = mat_mul(&proj, &mat_mul(&view, &model));
        surface.present(&mvp)?;
    }

    Ok(())
}
