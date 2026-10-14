//! Instance data layouts and pipeline descriptors for the unified-API painter.
//!
//! Each primitive kind draws a hardcoded 6-vertex quad (`corners[6]` in the
//! vertex shader, indexed by `gl_VertexIndex`) with one shader-pulled instance
//! per draw, selected via the `instances` range passed to
//! [`RenderCommands::draw`](zengpu_hal::RenderCommands::draw) (`first_instance`
//! = the element index, `instance_count` = 1) — so each binding uses
//! [`StepMode::Instance`].

use std::mem::size_of;

use crate::gpu::batch::DrawRef;
use zengpu::native::{NativeDevice, shader};
use zengpu_hal::{
    BlendMode, BlendState, ColorTargetState, DepthState, Format, GpuDevice, GraphicsDevice,
    GraphicsPipelineDesc, PipelineHandle, PrimitiveTopology, RasterState, Result, StepMode,
    VertexAttribute, VertexFormat, VertexLayout,
};
use zengpu_spirv::ZslShader;

use super::shaders::*;

/// One filled rectangle. `rect` is `[x, y, w, h]` in physical pixels; `color`
/// is straight RGBA. 32-byte `#[repr(C)]`, shared layout with [`ShapeInstance`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RectInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

/// One rounded box, filled or stroked. `rect` is `[x, y, w, h]`, `color`
/// straight RGBA, `shape` `[radius, stroke, _, _]`. 48-byte `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ShapeInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub shape: [f32; 4],
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
    /// Source size in texels, then 1 for nearest filtering.
    pub sampling: [f32; 4],
    pub affine: [f32; 4],
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

const _: () = assert!(size_of::<RectInstance>() == 32);
const _: () = assert!(size_of::<ShapeInstance>() == 48);
const _: () = assert!(size_of::<GradientInstance>() == 64);
const _: () = assert!(size_of::<ImageInstance>() == 96);
const _: () = assert!(size_of::<TextInstance>() == 48);

const fn float4(location: u32, offset: u32) -> VertexAttribute {
    VertexAttribute {
        location,
        offset,
        format: VertexFormat::Float32x4,
    }
}

const RECT_ATTRS: [VertexAttribute; 2] = [float4(0, 0), float4(1, 16)];
const SHAPE_ATTRS: [VertexAttribute; 3] = [float4(0, 0), float4(1, 16), float4(2, 32)];
const GRADIENT_ATTRS: [VertexAttribute; 3] = [float4(0, 0), float4(1, 16), float4(2, 32)];
const IMAGE_ATTRS: [VertexAttribute; 5] = [
    float4(0, 0),
    float4(1, 16),
    float4(2, 32),
    float4(3, 48),
    float4(4, 64),
];
const TEXT_ATTRS: [VertexAttribute; 2] = [float4(0, 0), float4(1, 16)];

const RECT_LAYOUT: VertexLayout = VertexLayout {
    stride: 32,
    attributes: &RECT_ATTRS,
    step_mode: StepMode::Instance,
};
const SHAPE_LAYOUT: VertexLayout = VertexLayout {
    stride: 48,
    attributes: &SHAPE_ATTRS,
    step_mode: StepMode::Instance,
};
const GRADIENT_LAYOUT: VertexLayout = VertexLayout {
    stride: 64,
    attributes: &GRADIENT_ATTRS,
    step_mode: StepMode::Instance,
};
const IMAGE_LAYOUT: VertexLayout = VertexLayout {
    stride: 96,
    attributes: &IMAGE_ATTRS,
    step_mode: StepMode::Instance,
};
const TEXT_LAYOUT: VertexLayout = VertexLayout {
    stride: 48,
    attributes: &TEXT_ATTRS,
    step_mode: StepMode::Instance,
};

/// Vertex and fragment SPIR-V, vertex layouts and blending for one pipeline.
type PipelineSpec<'s> = (
    &'s ZslShader,
    &'s ZslShader,
    &'s [VertexLayout<'s>],
    BlendMode,
);

/// All five pipelines the painter draws with, in painter-order priority.
#[derive(Clone, Copy, Default)]
pub struct Pipelines {
    pub rect: Option<PipelineHandle>,
    pub shape: Option<PipelineHandle>,
    pub gradient: Option<PipelineHandle>,
    pub image: Option<PipelineHandle>,
    pub text: Option<PipelineHandle>,
}

impl Pipelines {
    /// Create only the pipeline kinds used by this order, keeping successful
    /// creations for future frames and retries. Text uses
    /// [`BlendMode::DualSourceAlpha`] when the device supports it, falling
    /// back to [`BlendMode::AlphaBlend`] (coverage in `.a` only) otherwise.
    pub fn ensure(
        &mut self,
        device: &NativeDevice,
        color_format: Format,
        order: &[DrawRef],
    ) -> Result<()> {
        let text_dual_source = device.supports_dual_source_blending();
        let text_frag = if text_dual_source {
            &TEXT_DUAL_SOURCE_FRAG
        } else {
            &TEXT_FRAG
        };
        let text_blend = if text_dual_source {
            BlendMode::DualSourceAlpha
        } else {
            BlendMode::AlphaBlend
        };

        for draw in order {
            let (slot, spec): (&mut Option<PipelineHandle>, PipelineSpec<'_>) = match draw {
                DrawRef::Rect(_) => (
                    &mut self.rect,
                    (
                        &RECT_VERT,
                        &RECT_FRAG,
                        &[RECT_LAYOUT],
                        BlendMode::AlphaBlend,
                    ),
                ),
                DrawRef::Shape(_) => (
                    &mut self.shape,
                    (
                        &SHAPE_VERT,
                        &SHAPE_FRAG,
                        &[SHAPE_LAYOUT],
                        BlendMode::AlphaBlend,
                    ),
                ),
                DrawRef::Gradient(_) => (
                    &mut self.gradient,
                    (
                        &GRADIENT_VERT,
                        &GRADIENT_FRAG,
                        &[GRADIENT_LAYOUT],
                        BlendMode::AlphaBlend,
                    ),
                ),
                DrawRef::Image(_) => (
                    &mut self.image,
                    (
                        &IMAGE_VERT,
                        &IMAGE_FRAG,
                        &[IMAGE_LAYOUT],
                        BlendMode::AlphaBlend,
                    ),
                ),
                DrawRef::Text(_) => (
                    &mut self.text,
                    (&TEXT_VERT, text_frag, &[TEXT_LAYOUT], text_blend),
                ),
            };
            if slot.is_none() {
                let (vert, frag, layouts, blend) = spec;
                *slot = Some(create_pipeline(
                    device,
                    vert,
                    frag,
                    layouts,
                    blend,
                    color_format,
                )?);
            }
        }
        Ok(())
    }

    pub fn destroy(&self, device: &NativeDevice) {
        for pipeline in [self.rect, self.shape, self.gradient, self.image, self.text]
            .into_iter()
            .flatten()
        {
            device.destroy_pipeline(pipeline);
        }
    }
}

fn create_pipeline(
    device: &NativeDevice,
    vert: &ZslShader,
    frag: &ZslShader,
    vertex_layouts: &[VertexLayout],
    blend: BlendMode,
    color_format: Format,
) -> Result<PipelineHandle> {
    let vertex_shader = device.create_shader(shader(vert))?;
    let fragment_shader = match device.create_shader(shader(frag)) {
        Ok(shader) => shader,
        Err(e) => {
            device.destroy_shader(vertex_shader);
            return Err(e);
        }
    };
    let blend = match blend {
        BlendMode::Opaque => None,
        BlendMode::AlphaBlend => Some(BlendState::ALPHA_BLEND),
        BlendMode::DualSourceAlpha => Some(BlendState::DUAL_SOURCE_ALPHA),
    };
    let pipeline = device.create_graphics_pipeline(GraphicsPipelineDesc {
        vertex_shader,
        fragment_shader,
        vertex_layouts,
        topology: PrimitiveTopology::TriangleList,
        color_targets: &[ColorTargetState {
            format: color_format,
            blend,
        }],
        depth_format: None,
        depth: DepthState::default(),
        stencil: None,
        raster: RasterState::default(),
        samples: 1,
    });
    device.destroy_shader(vertex_shader);
    device.destroy_shader(fragment_shader);
    pipeline
}
