//! SPIR-V shaders for the unified-API painter.

use inline_spirv::inline_spirv;
use std::mem::size_of_val;
use std::slice::from_raw_parts;

/// View SPIR-V words as the bytes [`zengpu_hal::ShaderDesc`] expects.
pub fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { from_raw_parts(words.as_ptr() as *const u8, size_of_val(words)) }
}

pub const RECT_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 i_rect;   // x, y, w, h  (physical pixels)
    layout(location = 1) in vec4 i_color;  // straight RGBA
    layout(push_constant) uniform PC { vec2 viewport; } pc;
    layout(location = 0) out vec4 v_color;
    void main() {
        vec2 corners[6] = vec2[](
            vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
            vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
        );
        vec2 corner = corners[gl_VertexIndex];
        vec2 px = i_rect.xy + corner * i_rect.zw;
        // Vulkan NDC: top-left is (-1, -1), +y points down — matches pixel space.
        vec2 ndc = (px / pc.viewport) * 2.0 - 1.0;
        gl_Position = vec4(ndc, 0.0, 1.0);
        v_color = i_color;
    }
    "#,
    vert,
    vulkan1_0
);

pub const RECT_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 v_color;
    layout(location = 0) out vec4 o_color;
    void main() { o_color = v_color; }
    "#,
    frag,
    vulkan1_0
);

pub const CIRCLE_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 i_data;   // cx, cy, radius, _
    layout(location = 1) in vec4 i_color;  // straight RGBA
    layout(push_constant) uniform PC { vec2 viewport; } pc;
    layout(location = 0) out vec2 v_local;   // offset from centre (px)
    layout(location = 1) out vec4 v_color;
    layout(location = 2) out float v_radius;
    void main() {
        vec2 corners[6] = vec2[](
            vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
            vec2( 1.0, -1.0), vec2(1.0,  1.0), vec2(-1.0, 1.0)
        );
        vec2 corner = corners[gl_VertexIndex];
        float r = i_data.z;
        vec2 px = i_data.xy + corner * r;
        v_local = corner * r;
        v_radius = r;
        v_color = i_color;
        vec2 ndc = (px / pc.viewport) * 2.0 - 1.0;
        gl_Position = vec4(ndc, 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

pub const CIRCLE_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec2 v_local;
    layout(location = 1) in vec4 v_color;
    layout(location = 2) in float v_radius;
    layout(location = 0) out vec4 o_color;
    void main() {
        float dist = length(v_local);
        float alpha = 1.0 - smoothstep(v_radius - 1.0, v_radius, dist);
        if (alpha <= 0.0) discard;
        o_color = vec4(v_color.rgb, v_color.a * alpha);
    }
    "#,
    frag,
    vulkan1_0
);

pub const GRADIENT_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 i_rect;    // x, y, w, h (fill area, px)
    layout(location = 1) in vec4 i_a;        // linear start.xy / radial centre.xy,.z=r,.w=kind
    layout(location = 2) in vec4 i_b;        // linear end.xy
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) out vec2 v_px;
    layout(location = 1) out vec4 v_a;
    layout(location = 2) out vec4 v_b;
    void main() {
        vec2 corners[6] = vec2[](
            vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
            vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
        );
        vec2 corner = corners[gl_VertexIndex];
        vec2 px = i_rect.xy + corner * i_rect.zw;
        v_px = px; v_a = i_a; v_b = i_b;
        vec2 ndc = (px / pc.viewport) * 2.0 - 1.0;
        gl_Position = vec4(ndc, 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

pub const GRADIENT_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 1) uniform sampler2D textures[1024];
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) in vec2 v_px;
    layout(location = 1) in vec4 v_a;
    layout(location = 2) in vec4 v_b;
    layout(location = 0) out vec4 o_color;
    void main() {
        float t;
        if (v_a.w < 0.5) {
            vec2 d = v_b.xy - v_a.xy;
            t = dot(v_px - v_a.xy, d) / max(dot(d, d), 1e-6);
        } else {
            t = length(v_px - v_a.xy) / max(v_a.z, 1e-6);
        }
        float lut_u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0;
        o_color = texture(textures[pc.slot], vec2(lut_u, 0.5));
    }
    "#,
    frag,
    vulkan1_0
);

pub const IMAGE_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 i_rect;   // dest x, y, w, h (px)
    layout(location = 1) in vec4 i_uv;     // u0, v0, u1, v1
    layout(location = 2) in vec4 i_tint;
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) out vec2 v_uv;
    layout(location = 1) out vec4 v_tint;
    void main() {
        vec2 corners[6] = vec2[](
            vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
            vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
        );
        vec2 corner = corners[gl_VertexIndex];
        vec2 px = i_rect.xy + corner * i_rect.zw;
        v_uv = mix(i_uv.xy, i_uv.zw, corner);
        v_tint = i_tint;
        vec2 ndc = (px / pc.viewport) * 2.0 - 1.0;
        gl_Position = vec4(ndc, 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

pub const IMAGE_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 1) uniform sampler2D textures[1024];
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) in vec2 v_uv;
    layout(location = 1) in vec4 v_tint;
    layout(location = 0) out vec4 o_color;
    void main() {
        o_color = texture(textures[pc.slot], v_uv) * v_tint;
    }
    "#,
    frag,
    vulkan1_0
);

pub const TEXT_VERT_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(location = 0) in vec4 i_rect;
    layout(location = 1) in vec4 i_color;
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) out vec2 v_uv;
    layout(location = 1) out vec4 v_color;
    void main() {
        vec2 corners[6] = vec2[](
            vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
            vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
        );
        vec2 corner = corners[gl_VertexIndex];
        vec2 px = i_rect.xy + corner * i_rect.zw;
        v_uv = corner;
        v_color = i_color;
        vec2 ndc = (px / pc.viewport) * 2.0 - 1.0;
        gl_Position = vec4(ndc, 0.0, 1.0);
    }
    "#,
    vert,
    vulkan1_0
);

pub const TEXT_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 1) uniform sampler2D textures[1024];
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) in vec2 v_uv;
    layout(location = 1) in vec4 v_color;
    layout(location = 0) out vec4 o_color;
    void main() {
        vec4 coverage = texture(textures[pc.slot], v_uv);
        float alpha = coverage.a * v_color.a;
        if (alpha <= 0.0) discard;
        o_color = vec4(v_color.rgb, alpha);
    }
    "#,
    frag,
    vulkan1_0
);

pub const TEXT_DUAL_SOURCE_FRAG_SPV: &[u32] = inline_spirv!(
    r#"
    #version 450
    layout(set = 0, binding = 1) uniform sampler2D textures[1024];
    layout(push_constant) uniform PC { vec2 viewport; uint slot; } pc;
    layout(location = 0) in vec2 v_uv;
    layout(location = 1) in vec4 v_color;
    layout(location = 0, index = 0) out vec4 o_color;
    layout(location = 0, index = 1) out vec4 o_coverage;
    void main() {
        vec3 coverage = texture(textures[pc.slot], v_uv).rgb * v_color.a;
        float alpha = max(coverage.r, max(coverage.g, coverage.b));
        if (alpha <= 0.0) discard;
        o_color = vec4(v_color.rgb, 1.0);
        o_coverage = vec4(coverage, alpha);
    }
    "#,
    frag,
    vulkan1_0
);
