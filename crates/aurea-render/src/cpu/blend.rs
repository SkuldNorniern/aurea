//! Pixel-level blend modes for compositing.
//!
//! Colors are packed as RGBA u32: alpha in the high byte, then red, green, blue.
//! Each mode combines a source pixel with the existing destination pixel to produce
//! the final color (e.g. Normal is "over", Multiply darkens, Screen lightens).

use super::super::types::BlendMode;

/// Composites a source pixel onto a destination pixel using the given blend mode.
/// Returns the resulting color as RGBA u32.
pub fn blend_pixel(src: u32, dst: u32, mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => blend_over(src, dst),
        BlendMode::Multiply => blend_multiply(src, dst),
        BlendMode::Screen => blend_screen(src, dst),
        BlendMode::Overlay => blend_overlay(src, dst),
        BlendMode::Darken => blend_darken(src, dst),
        BlendMode::Lighten => blend_lighten(src, dst),
        BlendMode::ColorDodge => blend_color_dodge(src, dst),
        BlendMode::ColorBurn => blend_color_burn(src, dst),
        BlendMode::HardLight => blend_hard_light(src, dst),
        BlendMode::SoftLight => blend_soft_light(src, dst),
        BlendMode::Difference => blend_difference(src, dst),
        BlendMode::Exclusion => blend_exclusion(src, dst),
    }
}

fn sr(src: u32) -> u32 {
    (src >> 16) & 0xff
}
fn sg(src: u32) -> u32 {
    (src >> 8) & 0xff
}
fn sb(src: u32) -> u32 {
    src & 0xff
}
fn sa(src: u32) -> u32 {
    (src >> 24) & 0xff
}
fn dr(dst: u32) -> u32 {
    (dst >> 16) & 0xff
}
fn dg(dst: u32) -> u32 {
    (dst >> 8) & 0xff
}
fn db(dst: u32) -> u32 {
    dst & 0xff
}
fn da(dst: u32) -> u32 {
    (dst >> 24) & 0xff
}

fn blend_over(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    if sa >= 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let da = da(dst);
    let inv_sa = 255 - sa;
    let out_a = sa + (inv_sa * da) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = (sa * sr(src) + inv_sa * dr(dst)) / 255;
    let out_g = (sa * sg(src) + inv_sa * dg(dst)) / 255;
    let out_b = (sa * sb(src) + inv_sa * db(dst)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_multiply(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = (sa * da) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = (sr(src) * dr(dst)) / 255;
    let out_g = (sg(src) * dg(dst)) / 255;
    let out_b = (sb(src) * db(dst)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_screen(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = 255 - ((255 - sr) * (255 - dr)) / 255;
    let out_g = 255 - ((255 - sg) * (255 - dg)) / 255;
    let out_b = 255 - ((255 - sb) * (255 - db)) / 255;
    let out_a = 255 - ((255 - sa) * (255 - da)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_overlay(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = overlay_channel(sr, dr);
    let out_g = overlay_channel(sg, dg);
    let out_b = overlay_channel(sb, db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn overlay_channel(s: u32, d: u32) -> u32 {
    if d < 128 {
        (2 * s * d) / 255
    } else {
        255 - (2 * (255 - s) * (255 - d)) / 255
    }
}

fn blend_darken(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = sa + (da * (255 - sa)) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = sr(src).min(dr(dst));
    let out_g = sg(src).min(dg(dst));
    let out_b = sb(src).min(db(dst));
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_lighten(src: u32, dst: u32) -> u32 {
    let sa = sa(src);
    let da = da(dst);
    let out_a = sa + (da * (255 - sa)) / 255;
    if out_a == 0 {
        return 0;
    }
    let out_r = sr(src).max(dr(dst));
    let out_g = sg(src).max(dg(dst));
    let out_b = sb(src).max(db(dst));
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_color_dodge(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = if sr >= 255 {
        255
    } else {
        (255 * dr).min(255) / (255 - sr)
    };
    let out_g = if sg >= 255 {
        255
    } else {
        (255 * dg).min(255) / (255 - sg)
    };
    let out_b = if sb >= 255 {
        255
    } else {
        (255 * db).min(255) / (255 - sb)
    };
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_color_burn(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = if sr == 0 {
        0
    } else {
        255 - ((255 - dr) * 255).min(255) / sr
    };
    let out_g = if sg == 0 {
        0
    } else {
        255 - ((255 - dg) * 255).min(255) / sg
    };
    let out_b = if sb == 0 {
        0
    } else {
        255 - ((255 - db) * 255).min(255) / sb
    };
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_hard_light(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = overlay_channel(dr, sr);
    let out_g = overlay_channel(dg, sg);
    let out_b = overlay_channel(db, sb);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_soft_light(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = soft_light_channel(sr, dr);
    let out_g = soft_light_channel(sg, dg);
    let out_b = soft_light_channel(sb, db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn soft_light_channel(s: u32, d: u32) -> u32 {
    let s = s as f32 / 255.0;
    let d = d as f32 / 255.0;
    let r = if s <= 0.5 {
        d * (1.0 - (1.0 - 2.0 * s) * (1.0 - d))
    } else {
        d * (1.0
            + (2.0 * s - 1.0)
                * (if d <= 0.25 {
                    ((16.0 * d - 12.0) * d + 4.0) * d
                } else {
                    d.sqrt() - d
                }))
    };
    (r * 255.0).clamp(0.0, 255.0) as u32
}

fn blend_difference(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = sr.abs_diff(dr);
    let out_g = sg.abs_diff(dg);
    let out_b = sb.abs_diff(db);
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

fn blend_exclusion(src: u32, dst: u32) -> u32 {
    let sr = sr(src);
    let sg = sg(src);
    let sb = sb(src);
    let sa = sa(src);
    let dr = dr(dst);
    let dg = dg(dst);
    let db = db(dst);
    let da = da(dst);
    let out_r = sr + dr - (2 * sr * dr) / 255;
    let out_g = sg + dg - (2 * sg * dg) / 255;
    let out_b = sb + db - (2 * sb * db) / 255;
    let out_a = sa + (da * (255 - sa)) / 255;
    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}
