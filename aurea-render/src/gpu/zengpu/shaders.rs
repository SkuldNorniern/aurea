//! The painter's shaders, in ZSL.

use zengpu_spirv::{ZslShader, zsl};

pub const RECT_VERT: ZslShader = zsl!(
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
);

pub const RECT_FRAG: ZslShader = zsl!(
    fragment rect_fs(@location(0) color: f32x4) -> f32x4 {
        color
    }
);

pub const SHAPE_VERT: ZslShader = zsl!(
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
);

pub const SHAPE_FRAG: ZslShader = zsl!(
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
);

pub const GRADIENT_VERT: ZslShader = zsl!(
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
);

pub const GRADIENT_FRAG: ZslShader = zsl!(
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
);

pub const IMAGE_VERT: ZslShader = zsl!(
    push Pc { viewport: f32x2 }
    vertex image_vs(
        @location(0) rect: f32x4,
        @location(1) uv: f32x4,
        @location(2) tint: f32x4,
        @location(3) sampling: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), mix(uv.xy, uv.zw, corner), tint, sampling)
    }
);

pub const IMAGE_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment image_fs(
        @location(0) uv: f32x2,
        @location(1) tint: f32x4,
        @location(2) sampling: f32x4,
        tex: textures,
        pc: Pc,
    ) -> f32x4 {
        let at = uv
        if sampling.z > 0.5 {
            at = (floor(uv * sampling.xy) + 0.5) / sampling.xy
        }
        tex[pc.slot].sample(at) * tint
    }
);

pub const TEXT_VERT: ZslShader = zsl!(
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
);

pub const TEXT_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, slot: u32 }
    fragment text_fs(@location(0) uv: f32x2, @location(1) color: f32x4, tex: textures, pc: Pc) -> f32x4 {
        let alpha = tex[pc.slot].sample(uv).a * color.a
        if alpha <= 0.0 {
            discard
        }
        f32x4(color.rgb, alpha)
    }
);

pub const TEXT_DUAL_SOURCE_FRAG: ZslShader = zsl!(
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
);

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
