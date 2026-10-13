//! Instance data layouts and pipeline descriptors for the unified-API painter.

use std::mem::size_of;

use zengpu::native::{NativeDevice, shader};
use zengpu_hal::{
    BlendMode, BlendState, ColorTargetState, DepthState, Format, GpuDevice, GpuError,
    GraphicsDevice, GraphicsPipelineDesc, PipelineHandle, PrimitiveTopology, RasterState, Result,
    StepMode, VertexAttribute, VertexFormat, VertexLayout,
};
use zengpu_spirv::ZslShader;

use super::shaders::*;

/// One filled rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RectInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

/// One rounded box, filled or stroked.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ShapeInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub shape: [f32; 4],
}

/// One gradient-filled rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GradientInstance {
    pub rect: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub slot: u32,
    pub _pad: [u32; 3],
}

/// One textured image quad.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ImageInstance {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub tint: [f32; 4],
    /// Source size in texels, then 1 for nearest filtering.
    pub sampling: [f32; 4],
    pub slot: u32,
    pub _pad: [u32; 3],
}

/// One text-run coverage quad.
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
const _: () = assert!(size_of::<ImageInstance>() == 80);
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
const IMAGE_ATTRS: [VertexAttribute; 4] =
    [float4(0, 0), float4(1, 16), float4(2, 32), float4(3, 48)];
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
    stride: 80,
    attributes: &IMAGE_ATTRS,
    step_mode: StepMode::Instance,
};
const TEXT_LAYOUT: VertexLayout = VertexLayout {
    stride: 48,
    attributes: &TEXT_ATTRS,
    step_mode: StepMode::Instance,
};

type PipelineSpec<'s> = (
    &'s ZslShader,
    &'s ZslShader,
    &'s [VertexLayout<'s>],
    BlendMode,
);

/// All five pipelines the painter draws with, in painter-order priority.
pub struct Pipelines {
    pub rect: PipelineHandle,
    pub shape: PipelineHandle,
    pub gradient: PipelineHandle,
    pub image: PipelineHandle,
    pub text: PipelineHandle,
}

impl Pipelines {
    /// Create all five pipelines for `color_format`.
    pub fn new(device: &NativeDevice, color_format: Format) -> Result<Self> {
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

        let make = |vert, frag, vertex_layouts, blend| {
            create_pipeline(device, vert, frag, vertex_layouts, blend, color_format)
        };

        let specs: [PipelineSpec<'_>; 5] = [
            (
                &RECT_VERT,
                &RECT_FRAG,
                &[RECT_LAYOUT],
                BlendMode::AlphaBlend,
            ),
            (
                &SHAPE_VERT,
                &SHAPE_FRAG,
                &[SHAPE_LAYOUT],
                BlendMode::AlphaBlend,
            ),
            (
                &GRADIENT_VERT,
                &GRADIENT_FRAG,
                &[GRADIENT_LAYOUT],
                BlendMode::AlphaBlend,
            ),
            (
                &IMAGE_VERT,
                &IMAGE_FRAG,
                &[IMAGE_LAYOUT],
                BlendMode::AlphaBlend,
            ),
            (&TEXT_VERT, text_frag, &[TEXT_LAYOUT], text_blend),
        ];
        let mut made = Vec::with_capacity(specs.len());
        for (vert, frag, layouts, blend) in specs {
            match make(vert, frag, layouts, blend) {
                Ok(pipeline) => made.push(pipeline),
                Err(e) => {
                    for pipeline in made {
                        device.destroy_pipeline(pipeline);
                    }
                    return Err(e);
                }
            }
        }
        match made[..] {
            [rect, shape, gradient, image, text] => Ok(Self {
                rect,
                shape,
                gradient,
                image,
                text,
            }),
            _ => Err(GpuError::PipelineCreation("pipeline count".into())),
        }
    }

    pub fn destroy(&self, device: &NativeDevice) {
        for pipeline in [self.rect, self.shape, self.gradient, self.image, self.text] {
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
