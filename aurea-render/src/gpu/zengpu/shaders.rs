//! The painter's shaders, in ZSL.

use zengpu_spirv::{ZslShader, zsl};

/// Integrates flattened path edges over each pixel on the GPU. Each edge adds
/// the area to its right; winding/even-odd coverage is then quantized per path,
/// matching the CPU clip stack's order of intersections.
pub const CLIP_MASK: ZslShader = zsl!(
    push P { paths: u32 }
    @workgroup_size(8, 8)
    kernel clip_mask(edges: device buffer<f32>, dst: image, p: P, id: global_id) {
        let x = id.x
        let y = id.y
        if x < dst.width() && y < dst.height() {
            let at: u32 = 0
            let coverage = 255.0
            for path in 0..p.paths {
                let count = u32(edges[at])
                let even = edges[at + 1]
                at = at + 2
                let sum = 0.0
                for i in 0..count {
                    let e = at + i * 5
                    let top = max(f32(y), edges[e])
                    let bottom = min(f32(y + 1), edges[e + 1])
                    if bottom > top {
                        let height = bottom - top
                        let a = f32(x + 1) - (edges[e + 2] + (top - edges[e]) * edges[e + 3])
                        let b = f32(x + 1) - (edges[e + 2] + (bottom - edges[e]) * edges[e + 3])
                        let area = 0.0
                        if a >= 1.0 && b >= 1.0 {
                            area = height
                        } else {
                            if a > 0.0 || b > 0.0 {
                                if abs(b - a) < 0.000001 {
                                    area = height * clamp((a + b) * 0.5, 0.0, 1.0)
                                } else {
                                    let ha = 0.0
                                    let hb = 0.0
                                    if a >= 1.0 { ha = a - 0.5 } else { if a > 0.0 { ha = a * a * 0.5 } }
                                    if b >= 1.0 { hb = b - 0.5 } else { if b > 0.0 { hb = b * b * 0.5 } }
                                    area = height * (hb - ha) / (b - a)
                                }
                            }
                        }
                        sum = sum + area * edges[e + 4]
                    }
                }
                at = at + count * 5
                let cov = min(abs(sum), 1.0)
                if even > 0.5 {
                    let folded = abs(sum) - floor(abs(sum) * 0.5) * 2.0
                    cov = 1.0 - abs(folded - 1.0)
                }
                let mask = floor(clamp(cov, 0.0, 1.0) * 255.0 + 0.5)
                coverage = floor(coverage * mask / 255.0 + 0.5)
            }
            dst.store(x, y, 1.0, 1.0, 1.0, coverage / 255.0)
        }
    }
);

pub const RECT_VERT: ZslShader = zsl!(
    push Pc { viewport: f32x2 }
    vertex rect_vs(
        @location(0) rect: f32x4,
        @location(1) color: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x4, f32x2) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), color, px)
    }
);

pub const RECT_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment rect_fs(@location(0) color: f32x4, @location(1) px: f32x2, tex: textures, pc: Pc) -> f32x4 {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = tex[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
        f32x4(color.rgb, color.a * clip_cov)
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
    ) -> (f32x4, f32x2, f32x4, f32x2, f32x3, f32x2) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1)) * 2.0 - 1.0
        let half_size = rect.zw * 0.5
        let local = corner * (half_size + (shape.y * 0.5 + 1.0 + shape.z * 3.0))
        let px = rect.xy + half_size + local
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), local, color, half_size, shape.xyz, px)
    }
);

pub const SHAPE_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment shape_fs(
        @location(0) local: f32x2,
        @location(1) color: f32x4,
        @location(2) half_size: f32x2,
        @location(3) shape: f32x3,
        @location(4) px: f32x2,
        tex: textures, pc: Pc,
    ) -> f32x4 {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = tex[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
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
        f32x4(color.rgb, color.a * alpha * clip_cov)
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
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment gradient_fs(
        @location(0) px: f32x2,
        @location(1) a: f32x4,
        @location(2) b: f32x4,
        lut: textures,
        pc: Pc,
    ) -> f32x4 {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = lut[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
        let t = 0.0
        if a.w < 0.5 {
            let d = b.xy - a.xy
            t = dot(px - a.xy, d) / max(dot(d, d), 1e-6)
        } else {
            t = length(px - a.xy) / max(a.z, 1e-6)
        }
        let u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0
        let color = lut[pc.slot].sample(f32x2(u, 0.5))
        f32x4(color.rgb, color.a * clip_cov)
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
    ) -> (f32x4, f32x2, f32x4, f32x4, f32x2) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw + f32x2(corner.y * affine.y, corner.x * affine.x)
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), mix(uv.xy, uv.zw, corner), tint, sampling, px)
    }
);

pub const IMAGE_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment image_fs(
        @location(0) uv: f32x2,
        @location(1) tint: f32x4,
        @location(2) sampling: f32x4,
        @location(3) px: f32x2,
        tex: textures,
        pc: Pc,
    ) -> f32x4 {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = tex[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
        let at = uv
        if sampling.z > 0.5 {
            at = (floor(uv * sampling.xy) + 0.5) / sampling.xy
        }
        let color = tex[pc.slot].sample_level(at, 0.0)
        if sampling.w > 0.5 {
            color = tex[pc.slot].sample(at)
        }
        let painted = color * tint
        f32x4(painted.rgb, painted.a * clip_cov)
    }
);

pub const TEXT_VERT: ZslShader = zsl!(
    push Pc { viewport: f32x2 }
    vertex text_vs(
        @location(0) rect: f32x4,
        @location(1) color: f32x4,
        vi: vertex_index,
        pc: Pc,
    ) -> (f32x4, f32x2, f32x4, f32x2) {
        let corner = f32x2(f32((26 >> vi) & 1), f32((52 >> vi) & 1))
        let px = rect.xy + corner * rect.zw
        (f32x4(px / pc.viewport * 2.0 - 1.0, 0.0, 1.0), corner, color, px)
    }
);

pub const TEXT_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment text_fs(@location(0) uv: f32x2, @location(1) color: f32x4, @location(2) px: f32x2, tex: textures, pc: Pc) -> f32x4 {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = tex[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
        let alpha = tex[pc.slot].sample(uv).a * color.a * clip_cov
        if alpha <= 0.0 {
            discard
        }
        f32x4(color.rgb, alpha)
    }
);

pub const TEXT_DUAL_SOURCE_FRAG: ZslShader = zsl!(
    push Pc { viewport: f32x2, mask_origin: f32x2, mask_size: f32x2, mask_on: u32, mask_slot: u32, slot: u32 }
    fragment text_lcd_fs(@location(0) uv: f32x2, @location(1) color: f32x4, @location(2) px: f32x2, tex: textures, pc: Pc)
        -> (@location(0) @blend_src(0) f32x4, @location(0) @blend_src(1) f32x4)
    {
        let clip_cov = 1.0
        if pc.mask_on != 0 {
            clip_cov = tex[pc.mask_slot].sample_level((px - pc.mask_origin) / pc.mask_size, 0.0).a
        }
        if clip_cov <= 0.0 { discard }
        let coverage = tex[pc.slot].sample(uv).rgb * color.a * clip_cov
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

    const SHADERS: [(&str, &ZslShader); 13] = [
        ("clip_mask", &CLIP_MASK),
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
