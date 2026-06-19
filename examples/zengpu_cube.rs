//! ZenGPU 3D cube rendered into an aurea window — unified graphics API.
//!
//! Depth-tested, indexed mesh with a per-vertex color and a push-constant MVP
//! matrix. No raw `ash`/`vk` types: surface, pipeline, buffers, and recording
//! all go through `zengpu_hal::{GraphicsDevice, Surface, RenderCommands}`. The
//! depth attachment is a `DepthTarget` registered as a `TargetHandle` via
//! `VulkanDevice::register_depth_target`.
//!
//! Run: cargo run --example zengpu_cube --features zengpu

#[cfg(not(feature = "zengpu"))]
use std::process::exit;
use std::{error::Error, result::Result as StdResult};
#[cfg(feature = "zengpu")]
use {
    aurea::{Window, WindowEvent},
    core::array::from_fn,
    inline_spirv::inline_spirv,
    std::{
        mem::{size_of, size_of_val},
        slice::from_raw_parts,
        time::Instant,
    },
    zengpu::{
        Acquire, Bindings, BlendMode, BufferDesc, BufferUsage, ColorAttachment, DepthAttachment,
        DepthState, DepthTarget, Format, Frame, GpuAdapter, GpuDevice, GpuError, GraphicsDevice,
        GraphicsPipelineDesc, LoadOp, MemoryUsage, PresentMode, PrimitiveTopology, Rect,
        RenderCommands, RenderPassDesc, Result, Scalar, ShaderDesc, Surface, SurfaceConfig,
        VertexAttribute, VertexFormat, VertexLayout, Viewport, ViewportScissor, VulkanInstance,
        WindowHandles,
    },
};

// ── Geometry ──────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
#[cfg(feature = "zengpu")]
struct Vertex3d {
    pos: [f32; 3],
    color: [f32; 3],
}

#[cfg(feature = "zengpu")]
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
#[cfg(feature = "zengpu")]
const CUBE_INDICES: [u32; 36] = [
    4, 5, 6,  4, 6, 7,
    1, 0, 3,  1, 3, 2,
    0, 4, 7,  0, 7, 3,
    5, 1, 2,  5, 2, 6,
    3, 7, 6,  3, 6, 2,
    0, 1, 5,  0, 5, 4,
];

// ── Column-major mat4 helpers ─────────────────────────────────────────────────

#[cfg(feature = "zengpu")]
type Mat4 = [f32; 16];

#[cfg(feature = "zengpu")]
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

#[cfg(feature = "zengpu")]
fn identity() -> Mat4 {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

#[cfg(feature = "zengpu")]
fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

#[cfg(feature = "zengpu")]
fn rotate_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[8] = s;
    m[2] = -s;
    m[10] = c;
    m
}

#[cfg(feature = "zengpu")]
fn rotate_x(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[5] = c;
    m[9] = -s;
    m[6] = s;
    m[10] = c;
    m
}

/// Standard right-handed perspective. The viewport's negative height (set in
/// `main`'s render loop) flips Y for Vulkan's +Y-down NDC, so no manual Y-flip
/// is needed here.
#[cfg(feature = "zengpu")]
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

#[cfg(feature = "zengpu")]
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

#[cfg(feature = "zengpu")]
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

/// View SPIR-V words as the bytes [`ShaderDesc`] expects.
#[cfg(feature = "zengpu")]
fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { from_raw_parts(words.as_ptr() as *const u8, size_of_val(words)) }
}

#[cfg(feature = "zengpu")]
fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { from_raw_parts(slice.as_ptr() as *const u8, size_of_val(slice)) }
}

// ── Event loop ────────────────────────────────────────────────────────────────

#[cfg(feature = "zengpu")]
fn run() -> Result<()> {
    let window = Window::new("ZenGPU — 3D Cube", 800, 600)
        .map_err(|e| GpuError::Backend(format!("window: {e}")))?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst
        .request_vulkan_adapter()
        .ok_or_else(|| GpuError::Backend("no Vulkan adapter found".into()))?;
    eprintln!("ZenGPU: {}", adapter.info().name);
    let device = adapter.open_with_surface(zengpu::DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| GpuError::Backend(format!("window handle: {e:?}")))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };
    let surface = device.create_surface(&handles, config)?;

    let vert_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(VERT_SPV),
    })?;
    let frag_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(FRAG_SPV),
    })?;

    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader: vert_shader,
        fragment_shader: frag_shader,
        vertex_layouts: &[VertexLayout {
            stride: size_of::<Vertex3d>() as u32,
            attributes: &[
                VertexAttribute {
                    location: 0,
                    offset: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    location: 1,
                    offset: 12,
                    format: VertexFormat::Float32x3,
                },
            ],
            ..Default::default()
        }],
        topology: PrimitiveTopology::TriangleList,
        color_format: config.format,
        depth_format: Some(Format::Depth32Float),
        depth: DepthState {
            test: true,
            write: true,
        },
        blend: BlendMode::default(),
        samples: 1,
    })?;

    let vertices = cube_vertices();
    let vbytes = as_bytes(&vertices);
    let vertex_buf = device.create_buffer(BufferDesc {
        size: vbytes.len() as u64,
        usage: BufferUsage::VERTEX,
        memory: MemoryUsage::Upload,
    })?;
    device.write_buffer(vertex_buf, 0, vbytes)?;

    let ibytes = as_bytes(&CUBE_INDICES);
    let index_buf = device.create_buffer(BufferDesc {
        size: ibytes.len() as u64,
        usage: BufferUsage::INDEX,
        memory: MemoryUsage::Upload,
    })?;
    device.write_buffer(index_buf, 0, ibytes)?;

    let ctx = device.context();
    let mut depth = DepthTarget::new(&ctx, w.max(1), h.max(1))?;
    let mut depth_target = device.register_depth_target(&depth);

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

        let (sw, sh) = surface.size();
        if depth.extent() != (sw, sh) {
            device.unregister_render_target(depth_target);
            depth = DepthTarget::new(&ctx, sw.max(1), sh.max(1))?;
            depth_target = device.register_depth_target(&depth);
        }

        let frame = match surface.acquire()? {
            Acquire::Frame(frame) => frame,
            Acquire::Skip => continue,
        };

        let t = start.elapsed().as_secs_f32();
        let model = mat_mul(&rotate_y(t * 0.6), &rotate_x(t * 0.3));
        let view = translate(0.0, 0.0, -5.0);
        let proj = perspective(60f32.to_radians(), sw as f32 / sh.max(1) as f32, 0.1, 100.0);
        let mvp = mat_mul(&proj, &mat_mul(&view, &model));
        let scalars: [Scalar; 16] = from_fn(|i| Scalar::F32(mvp[i]));

        let mut list = device.create_command_list()?;
        list.begin_render_pass(&RenderPassDesc {
            color: &[ColorAttachment {
                target: frame.target(),
                load: LoadOp::clear_rgb(0.02, 0.02, 0.05),
                store: true,
                sample_after: false,
            }],
            depth: Some(DepthAttachment {
                target: depth_target,
                load: LoadOp::clear_depth(1.0),
                store: false,
            }),
        });
        list.set_pipeline(pipeline);
        list.set_viewport_scissor(ViewportScissor {
            viewport: Viewport {
                x: 0.0,
                y: sh as f32,
                width: sw as f32,
                height: -(sh as f32),
                min_depth: 0.0,
                max_depth: 1.0,
            },
            scissor: Some(Rect {
                x: 0.0,
                y: 0.0,
                width: sw as f32,
                height: sh as f32,
            }),
        });
        list.bind(Bindings {
            scalars: &scalars,
            ..Default::default()
        });
        list.set_vertex_buffer(0, vertex_buf);
        list.set_index_buffer(index_buf);
        list.draw_indexed(0..CUBE_INDICES.len() as u32, 0..1);
        list.end_render_pass();

        surface.present(frame, list)?;
    }

    Ok(())
}

fn main() -> StdResult<(), Box<dyn Error>> {
    #[cfg(not(feature = "zengpu"))]
    {
        eprintln!("This example requires the `zengpu` feature.");
        eprintln!("Run with: cargo run --example zengpu_cube --features zengpu");
        exit(1);
    }

    #[cfg(feature = "zengpu")]
    {
        run()?;
        Ok(())
    }
}
