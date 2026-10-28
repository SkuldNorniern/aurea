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
    ) -> (f32x4, f32x2, f32x4, f32x2, f32x3) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1)) * 2.0 - 1.0
        let half_size = rect.zw * 0.5
        let local = corner * (half_size + (shape.y * 0.5 + 1.0 + shape.z * 3.0))
        let px = rect.xy + half_size + local
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), local, color, half_size, shape.xyz)
    }
);

pub const SHAPE_FRAG: ZslShader = zsl!(
    fragment shape_fs(
        @location(0) local: f32x2,
        @location(1) color: f32x4,
        @location(2) half_size: f32x2,
        @location(3) shape: f32x3,
    ) -> f32x4 {
        let r = clamp(shape.x, 0.0, min(half_size.x, half_size.y))
        let q = abs(local) - half_size + r
        let d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r
        if shape.y > 0.0 {
            d = abs(d) - shape.y * 0.5
        }
        let alpha = clamp(0.5 - d, 0.0, 1.0)
        if shape.z > 0.0 {
            let x = abs(d) / (shape.z * 1.41421356)
            let t = 1.0 / (1.0 + 0.3275911 * x)
            let tail = 0.5 * t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429)))) * exp(-x * x)
            alpha = tail
            if d < 0.0 {
                alpha = 1.0 - tail
            }
        }
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
        @location(4) affine: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4, f32x4) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw + f32x2(corner.y * affine.y, corner.x * affine.x)
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
        let color = tex[pc.slot].sample_level(at, 0.0)
        if sampling.w > 0.5 {
            color = tex[pc.slot].sample(at)
        }
        color * tint
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

/// One texel of the next mip level: `area_downsample`'s span weights and alpha-weighted sums, in
/// the same order, so the rounding matches the CPU result. `src` is the level above `dst`.
pub const MIP_DOWN: ZslShader = zsl!(
    push P { rx: f32, ry: f32 }
    @workgroup_size(8, 8)
    kernel mip_down(src: image, dst: image, p: P, id: global_id) {
        let x = id.x
        let y = id.y
        if x < dst.width() && y < dst.height() {
            let rx = p.rx
            let ry = p.ry
            let lox = f32(x) * rx
            let hix = f32(x + 1) * rx
            let loy = f32(y) * ry
            let hiy = f32(y + 1) * ry
            let x0 = u32(floor(lox))
            let x1 = min(u32(ceil(hix)), src.width())
            let y0 = u32(floor(loy))
            let y1 = min(u32(ceil(hiy)), src.height())
            let s0 = 0.0
            let s1 = 0.0
            let s2 = 0.0
            let s3 = 0.0
            for ty in y0..y1 {
                // Divisions are corrected by a fused residual: a plain GPU `/` can be an ulp off
                // from IEEE, which moves a value across a rounding tie against the CPU.
                let wyn = max(min(hiy, f32(ty + 1)) - max(loy, f32(ty)), 0.0)
                let wyq = wyn / ry
                let wy = wyq + mul_add_f32(0.0 - wyq, ry, wyn) / ry
                for tx in x0..x1 {
                    let wxn = max(min(hix, f32(tx + 1)) - max(lox, f32(tx)), 0.0)
                    let wxq = wxn / rx
                    let wx = wxq + mul_add_f32(0.0 - wxq, rx, wxn) / rx
                    let pa = floor(src.load(tx, ty).a * 255.0 + 0.5)
                    let pr = floor(src.load(tx, ty).r * 255.0 + 0.5)
                    let pg = floor(src.load(tx, ty).g * 255.0 + 0.5)
                    let pb = floor(src.load(tx, ty).b * 255.0 + 0.5)
                    let a = pa * wx * wy
                    s0 = s0 + pr * a
                    s1 = s1 + pg * a
                    s2 = s2 + pb * a
                    s3 = s3 + a
                }
            }
            let r = 0.0
            let g = 0.0
            let b = 0.0
            if s3 > 0.0 {
                let qr0 = s0 / s3
                let qr = qr0 + mul_add_f32(0.0 - qr0, s3, s0) / s3
                let qg0 = s1 / s3
                let qg = qg0 + mul_add_f32(0.0 - qg0, s3, s1) / s3
                let qb0 = s2 / s3
                let qb = qb0 + mul_add_f32(0.0 - qb0, s3, s2) / s3
                let fr = floor(qr)
                let fg = floor(qg)
                let fb = floor(qb)
                r = fr
                g = fg
                b = fb
                if qr - fr >= 0.5 {
                    r = fr + 1.0
                }
                if qg - fg >= 0.5 {
                    g = fg + 1.0
                }
                if qb - fb >= 0.5 {
                    b = fb + 1.0
                }
            }
            let fa = floor(s3)
            let oa = fa
            if s3 - fa >= 0.5 {
                oa = fa + 1.0
            }
            dst.store(x, y, min(r, 255.0) / 255.0, min(g, 255.0) / 255.0, min(b, 255.0) / 255.0, min(oa, 255.0) / 255.0)
        }
    }
);

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs::{remove_file, write};
    use std::mem::size_of_val;
    use std::process::{Command, id};
    use std::slice::from_raw_parts;

    use super::*;

    const SHADERS: [(&str, &ZslShader); 12] = [
        ("rect.vert", &RECT_VERT),
        ("rect.frag", &RECT_FRAG),
        ("shape.vert", &SHAPE_VERT),
        ("shape.frag", &SHAPE_FRAG),
        ("gradient.vert", &GRADIENT_VERT),
        ("gradient.frag", &GRADIENT_FRAG),
        ("image.vert", &IMAGE_VERT),
        ("image.frag", &IMAGE_FRAG),
        ("text.vert", &TEXT_VERT),
        ("text.frag", &TEXT_FRAG),
        ("text_lcd.frag", &TEXT_DUAL_SOURCE_FRAG),
        ("mip_down", &MIP_DOWN),
    ];

    #[test]
    fn shaders_have_every_native_backend() {
        for (name, shader) in SHADERS {
            let spv = shader.spv.unwrap_or_else(|e| panic!("{name}: {e}"));
            let msl = shader.msl.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!spv.is_empty(), "{name}: no SPIR-V");
            assert!(!msl.is_empty(), "{name}: no MSL");
        }
    }

    #[test]
    fn shaders_validate() {
        for (name, shader) in SHADERS {
            let path = temp_dir().join(format!("aurea_{}_{name}.spv", id()));
            let spv = shader.spv.unwrap_or_else(|e| panic!("{name}: {e}"));
            // SAFETY: SPIR-V words viewed as their bytes.
            let bytes = unsafe { from_raw_parts(spv.as_ptr().cast::<u8>(), size_of_val(spv)) };
            write(&path, bytes).expect("write spv");
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
