pub const RECT_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// A rounded box, filled or stroked, evaluated as a signed distance field
/// over its bounding quad for a 1px-antialiased edge. Circles are boxes
/// whose radius is half their side.
pub const SHAPE_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) shape: vec2<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let half_size = instance.rect.zw * 0.5;
    // Room for the stroke past the outline and the antialiased edge.
    let reach = half_size + vec2<f32>(instance.shape.y * 0.5 + 1.0);
    let local = corner * reach;
    let px = instance.rect.xy + half_size + local;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.local = local;
    out.color = instance.color;
    out.half_size = half_size;
    out.shape = instance.shape.xy;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let r = clamp(in.shape.x, 0.0, min(in.half_size.x, in.half_size.y));
    let q = abs(in.local) - in.half_size + r;
    var d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    // A stroke is centred on the outline.
    if (in.shape.y > 0.0) {
        d = abs(d) - in.shape.y * 0.5;
    }
    let alpha = clamp(0.5 - d, 0.0, 1.0);
    if (alpha <= 0.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Computes a gradient parameter `t` per pixel and samples a 256x1 LUT texture
/// (group 1). The LUT is uploaded via `Gpu2dBackend::upload_image`; the slot
/// index is the bind group key in `WgpuBackend::slot_resources`.
pub const GRADIENT_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var lut_tex: texture_2d<f32>;
@group(1) @binding(1) var lut_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) a: vec4<f32>,
    @location(2) b: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) a: vec4<f32>,
    @location(2) b: vec4<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.px = px;
    out.a = instance.a;
    out.b = instance.b;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var t: f32;
    if (in.a.w < 0.5) {
        let d = in.b.xy - in.a.xy;
        t = dot(in.px - in.a.xy, d) / max(dot(d, d), 1e-6);
    } else {
        t = length(in.px - in.a.xy) / max(in.a.z, 1e-6);
    }
    let u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0;
    return textureSample(lut_tex, lut_sampler, vec2<f32>(u, 0.5));
}
"#;

/// Samples an RGBA image over its rect, `uv` picking the part of the
/// texture shown, tinted (for opacity) by `tint`.
pub const IMAGE_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) sampling: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) sampling: vec4<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    out.tint = instance.tint;
    out.sampling = instance.sampling;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Nearest: read a texel's centre, where linear filtering returns it whole.
    var uv = in.uv;
    if (in.sampling.z > 0.5) {
        uv = (floor(uv * in.sampling.xy) + 0.5) / in.sampling.xy;
    }
    return textureSample(image_tex, image_sampler, uv) * in.tint;
}
"#;

/// Draws a glyph run from its coverage mask: the mask's alpha is how much
/// of each pixel the text covers, painted in `color`.
pub const TEXT_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var mask_tex: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = corner;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(mask_tex, mask_sampler, in.uv).a;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga::front::wgsl;
    use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};

    /// WGSL is only compiled when a device makes its pipeline; checking it
    /// here catches a broken shader without a GPU.
    #[test]
    fn every_shader_parses_and_validates() {
        for (name, source) in [
            ("rect", RECT_SHADER),
            ("shape", SHAPE_SHADER),
            ("gradient", GRADIENT_SHADER),
            ("image", IMAGE_SHADER),
            ("text", TEXT_SHADER),
        ] {
            let module = wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {e}"));
            Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }
}
