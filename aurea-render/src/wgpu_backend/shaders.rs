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

/// Expands each instance's bounding-box quad and evaluates a signed distance
/// field in the fragment shader for a 1px-antialiased edge.
pub const CIRCLE_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vidx: u32, instance: Instance) -> VsOut {
    let corner = CORNERS[vidx];
    let r = instance.center_radius.z;
    let px = instance.center_radius.xy + corner * r;
    let ndc = (px / viewport.size) * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.local = corner * r;
    out.color = instance.color;
    out.radius = r;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dist = length(in.local);
    let alpha = 1.0 - smoothstep(in.radius - 1.0, in.radius, dist);
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
