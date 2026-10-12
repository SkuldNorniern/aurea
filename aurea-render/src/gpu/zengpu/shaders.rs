//! The painter's shaders, in ZSL.

use std::mem::size_of_val;
use std::slice::from_raw_parts;

use zengpu_spirv::zsl;

/// View SPIR-V words as the bytes [`zengpu_hal::ShaderDesc`] expects.
pub fn spv_bytes(words: &[u32]) -> &[u8] {
    unsafe { from_raw_parts(words.as_ptr() as *const u8, size_of_val(words)) }
}

pub const RECT_VERT_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2 }
    vertex rect_vs(
        @location(0) rect: f32x4,
        @location(1) color: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), color)
    }
)
.spv;

pub const RECT_FRAG_SPV: &[u32] = zsl!(
    fragment rect_fs(@location(0) color: f32x4) -> f32x4 {
        color
    }
)
.spv;

pub const SHAPE_VERT_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2 }
    vertex shape_vs(
        @location(0) rect: f32x4,
        @location(1) color: f32x4,
        @location(2) shape: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4, f32x2, f32x2) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1)) * 2.0 - 1.0
        let half_size = rect.zw * 0.5
        let local = corner * (half_size + (shape.y * 0.5 + 1.0))
        let px = rect.xy + half_size + local
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), local, color, half_size, shape.xy)
    }
)
.spv;

pub const SHAPE_FRAG_SPV: &[u32] = zsl!(
    fragment shape_fs(
        @location(0) local: f32x2,
        @location(1) color: f32x4,
        @location(2) half_size: f32x2,
        @location(3) shape: f32x2,
    ) -> f32x4 {
        let r = clamp(shape.x, 0.0, min(half_size.x, half_size.y))
        let q = abs(local) - half_size + r
        let d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r
        if shape.y > 0.0 {
            d = abs(d) - shape.y * 0.5
        }
        let alpha = clamp(0.5 - d, 0.0, 1.0)
        if alpha <= 0.0 {
            discard
        }
        f32x4(color.rgb, color.a * alpha)
    }
)
.spv;

pub const GRADIENT_VERT_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2 }
    vertex gradient_vs(
        @location(0) rect: f32x4,
        @location(1) a: f32x4,
        @location(2) b: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), px, a, b)
    }
)
.spv;

pub const GRADIENT_FRAG_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment gradient_fs(
        @location(0) px: f32x2,
        @location(1) a: f32x4,
        @location(2) b: f32x4,
        lut: textures,
        pc: Pc,
    ) -> f32x4 {
        let t = 0.0
        if a.w < 0.5 {
            let d = b.xy - a.xy
            t = dot(px - a.xy, d) / max(dot(d, d), 1e-6)
        } else {
            t = length(px - a.xy) / max(a.z, 1e-6)
        }
        let u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0
        lut[pc.slot].sample(f32x2(u, 0.5))
    }
)
.spv;

pub const IMAGE_VERT_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2 }
    vertex image_vs(
        @location(0) rect: f32x4,
        @location(1) uv: f32x4,
        @location(2) tint: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), mix(uv.xy, uv.zw, corner), tint)
    }
)
.spv;

pub const IMAGE_FRAG_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment image_fs(@location(0) uv: f32x2, @location(1) tint: f32x4, tex: textures, pc: Pc) -> f32x4 {
        tex[pc.slot].sample(uv) * tint
    }
)
.spv;

pub const TEXT_VERT_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2 }
    vertex text_vs(
        @location(0) rect: f32x4,
        @location(1) color: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), corner, color)
    }
)
.spv;

pub const TEXT_FRAG_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment text_fs(@location(0) uv: f32x2, @location(1) color: f32x4, tex: textures, pc: Pc) -> f32x4 {
        let alpha = tex[pc.slot].sample(uv).a * color.a
        if alpha <= 0.0 {
            discard
        }
        f32x4(color.rgb, alpha)
    }
)
.spv;

pub const TEXT_DUAL_SOURCE_FRAG_SPV: &[u32] = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment text_lcd_fs(@location(0) uv: f32x2, @location(1) color: f32x4, tex: textures, pc: Pc)
        -> (@location(0) @blend_src(0) f32x4, @location(0) @blend_src(1) f32x4)
    {
        let coverage = tex[pc.slot].sample(uv).rgb * color.a
        let alpha = max(coverage.r, max(coverage.g, coverage.b))
        if alpha <= 0.0 {
            discard
        }
        (f32x4(color.rgb, 1.0), f32x4(coverage, alpha))
    }
)
.spv;

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{remove_file, write};
    use std::process::{Command, id};

    use super::*;

    #[test]
    fn shaders_validate() {
        let shaders = [
            ("rect.vert", RECT_VERT_SPV),
            ("rect.frag", RECT_FRAG_SPV),
            ("shape.vert", SHAPE_VERT_SPV),
            ("shape.frag", SHAPE_FRAG_SPV),
            ("gradient.vert", GRADIENT_VERT_SPV),
            ("gradient.frag", GRADIENT_FRAG_SPV),
            ("image.vert", IMAGE_VERT_SPV),
            ("image.frag", IMAGE_FRAG_SPV),
            ("text.vert", TEXT_VERT_SPV),
            ("text.frag", TEXT_FRAG_SPV),
            ("text_lcd.frag", TEXT_DUAL_SOURCE_FRAG_SPV),
        ];
        for (name, words) in shaders {
            let path = temp_dir().join(format!("aurea_{}_{name}.spv", id()));
            write(&path, spv_bytes(words)).expect("write spv");
            let out = Command::new("spirv-val")
                .args(["--target-env", "vulkan1.2"])
                .arg(&path)
                .output();
            let _ = remove_file(&path);
            let Ok(out) = out else {
                return;
            };
            assert!(
                out.status.success(),
                "{name}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
