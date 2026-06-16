//! Instance data layouts and pipeline descriptors for the unified-API painter.
//!
//! Each primitive kind draws a hardcoded 6-vertex quad (`corners[6]` in the
//! vertex shader, indexed by `gl_VertexIndex`) with one shader-pulled instance
//! per draw, selected via the `instances` range passed to
//! [`RenderCommands::draw`](zengpu_hal::RenderCommands::draw) (`first_instance`
//! = the element index, `instance_count` = 1) — so each binding uses
//! [`StepMode::Instance`].

use zengpu_hal::{
    BlendMode, DepthState, Format, GpuDevice, GraphicsDevice, GraphicsPipelineDesc, PipelineHandle,
    PrimitiveTopology, Result, ShaderDesc, StepMode, VertexAttribute, VertexFormat, VertexLayout,
};
use zengpu_vulkan::VulkanDevice;

use super::shaders::*;

/// One filled rectangle. `rect` is `[x, y, w, h]` in physical pixels; `color`
/// is straight RGBA. 32-byte `#[repr(C)]`, shared layout with [`CircleInstance`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RectInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

/// One filled circle. `center_radius` is `[cx, cy, radius, _]`; `color` is
/// straight RGBA. 32-byte `#[repr(C)]`, shared layout with [`RectInstance`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CircleInstance {
    pub center_radius: [f32; 4],
    pub color: [f32; 4],
}

/// One gradient-filled rectangle. `a`/`b` encode linear (`a.w < 0.5`) or radial
/// (`a.w >= 0.5`) gradient parameters; `slot` selects the cached LUT texture in
/// the global bindless table (read CPU-side for the per-draw push constant —
/// the GPU ignores the vertex-buffer `slot`/`_pad` tail). 64-byte `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GradientInstance {
    pub rect: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub slot: u32,
    pub _pad: [u32; 3],
}

/// One textured image quad. `rect` is the dest `[x, y, w, h]` in physical
/// pixels; `uv` is the source region `[u0, v0, u1, v1]` (normalised); `tint` is
/// a straight-RGBA multiply; `slot` selects the bindless texture (read CPU-side
/// for the per-draw push constant). 64-byte `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ImageInstance {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub tint: [f32; 4],
    pub slot: u32,
    pub _pad: [u32; 3],
}

/// One text-run coverage quad. The bound texture stores RGB subpixel coverage
/// plus maximum coverage in alpha; `color` is the requested straight RGBA.
/// 48-byte `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TextInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub slot: u32,
    pub _pad: [u32; 3],
}

const _: () = assert!(std::mem::size_of::<RectInstance>() == 32);
const _: () = assert!(std::mem::size_of::<CircleInstance>() == 32);
const _: () = assert!(std::mem::size_of::<GradientInstance>() == 64);
const _: () = assert!(std::mem::size_of::<ImageInstance>() == 64);
const _: () = assert!(std::mem::size_of::<TextInstance>() == 48);

const fn float4(location: u32, offset: u32) -> VertexAttribute {
    VertexAttribute {
        location,
        offset,
        format: VertexFormat::Float32x4,
    }
}

const RECT_ATTRS: [VertexAttribute; 2] = [float4(0, 0), float4(1, 16)];
const GRADIENT_ATTRS: [VertexAttribute; 3] = [float4(0, 0), float4(1, 16), float4(2, 32)];
const IMAGE_ATTRS: [VertexAttribute; 3] = [float4(0, 0), float4(1, 16), float4(2, 32)];
const TEXT_ATTRS: [VertexAttribute; 2] = [float4(0, 0), float4(1, 16)];

const RECT_LAYOUT: VertexLayout = VertexLayout {
    stride: 32,
    attributes: &RECT_ATTRS,
    step_mode: StepMode::Instance,
};
const GRADIENT_LAYOUT: VertexLayout = VertexLayout {
    stride: 64,
    attributes: &GRADIENT_ATTRS,
    step_mode: StepMode::Instance,
};
const IMAGE_LAYOUT: VertexLayout = VertexLayout {
    stride: 64,
    attributes: &IMAGE_ATTRS,
    step_mode: StepMode::Instance,
};
const TEXT_LAYOUT: VertexLayout = VertexLayout {
    stride: 48,
    attributes: &TEXT_ATTRS,
    step_mode: StepMode::Instance,
};

/// All five pipelines the painter draws with, in painter-order priority.
pub struct Pipelines {
    pub rect: PipelineHandle,
    pub circle: PipelineHandle,
    pub gradient: PipelineHandle,
    pub image: PipelineHandle,
    pub text: PipelineHandle,
}

impl Pipelines {
    /// Create all five pipelines for `color_format`. Text uses
    /// [`BlendMode::DualSourceAlpha`] when the device supports it, falling
    /// back to [`BlendMode::AlphaBlend`] (coverage in `.a` only) otherwise.
    pub fn new(device: &VulkanDevice, color_format: Format) -> Result<Self> {
        let text_dual_source = device.supports_dual_source_blending();
        let text_frag_spv = if text_dual_source {
            TEXT_DUAL_SOURCE_FRAG_SPV
        } else {
            TEXT_FRAG_SPV
        };
        let text_blend = if text_dual_source {
            BlendMode::DualSourceAlpha
        } else {
            BlendMode::AlphaBlend
        };

        let rect = create_pipeline(
            device,
            RECT_VERT_SPV,
            RECT_FRAG_SPV,
            &[RECT_LAYOUT],
            BlendMode::AlphaBlend,
            color_format,
        )?;
        let circle = create_pipeline(
            device,
            CIRCLE_VERT_SPV,
            CIRCLE_FRAG_SPV,
            &[RECT_LAYOUT],
            BlendMode::AlphaBlend,
            color_format,
        )?;
        let gradient = create_pipeline(
            device,
            GRADIENT_VERT_SPV,
            GRADIENT_FRAG_SPV,
            &[GRADIENT_LAYOUT],
            BlendMode::AlphaBlend,
            color_format,
        )?;
        let image = create_pipeline(
            device,
            IMAGE_VERT_SPV,
            IMAGE_FRAG_SPV,
            &[IMAGE_LAYOUT],
            BlendMode::AlphaBlend,
            color_format,
        )?;
        let text = create_pipeline(
            device,
            TEXT_VERT_SPV,
            text_frag_spv,
            &[TEXT_LAYOUT],
            text_blend,
            color_format,
        )?;

        Ok(Self {
            rect,
            circle,
            gradient,
            image,
            text,
        })
    }
}

fn create_pipeline(
    device: &VulkanDevice,
    vert_spv: &[u32],
    frag_spv: &[u32],
    vertex_layouts: &[VertexLayout],
    blend: BlendMode,
    color_format: Format,
) -> Result<PipelineHandle> {
    let vertex_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(vert_spv),
    })?;
    let fragment_shader = device.create_shader(ShaderDesc {
        spirv: spv_bytes(frag_spv),
    })?;
    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader,
        fragment_shader,
        vertex_layouts,
        topology: PrimitiveTopology::TriangleList,
        color_format,
        depth_format: None,
        depth: DepthState::default(),
        blend,
        samples: 1,
    });
    device.destroy_shader(vertex_shader);
    device.destroy_shader(fragment_shader);
    pipeline
}
