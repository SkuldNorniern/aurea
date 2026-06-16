//! Resolves a lowered [`RenderBatches`] into a [`FramePlan`].
//!
//! Each textured batch kind (gradient LUT, image, text mask) is looked up or
//! uploaded via the backend-agnostic [`TextureCache`], which in turn calls
//! `Gpu2dBackend::upload_image`/`evict_image` for GPU work. The resulting
//! [`FramePlan`] carries per-kind instance arrays with shader slots already
//! assigned and the painter-order stream unchanged; the backend's
//! `present_frame` only needs to upload + record + present.

use aurea_foundation::{AureaError, AureaResult};

use crate::batch::{GradientInstance as BatchGradient, ImageDraw, RenderBatches, TextDraw};

use super::backend::Gpu2dBackend;
use super::frame_plan::{FramePlan, GradientPlanEntry, ImagePlanEntry, TextPlanEntry};
use super::texture_cache::TextureCache;

/// Fill `plan` from `batches`, resolving all texture slots through `cache`.
///
/// `plan` is fully overwritten; its backing `Vec`s are reused to avoid
/// per-frame allocation.
pub fn resolve_frame<B: Gpu2dBackend>(
    batches: &RenderBatches,
    cache: &mut TextureCache,
    backend: &mut B,
    frame: u64,
    plan: &mut FramePlan,
) -> AureaResult<()> {
    plan.reset();
    plan.clear = batches.clear;
    plan.order.extend_from_slice(&batches.order);

    resolve_gradients(&batches.gradients, cache, backend, frame, &mut plan.gradients)?;
    resolve_images(&batches.images, cache, backend, frame, &mut plan.images)?;
    resolve_texts(&batches.texts, cache, backend, frame, &mut plan.texts)?;

    Ok(())
}

fn resolve_gradients<B: Gpu2dBackend>(
    gradients: &[BatchGradient],
    cache: &mut TextureCache,
    backend: &mut B,
    frame: u64,
    out: &mut Vec<GradientPlanEntry>,
) -> AureaResult<()> {
    for g in gradients {
        let slot = cache.resolve(&g.lut, 256, 1, backend, frame)?;
        out.push(GradientPlanEntry { rect: g.rect, a: g.a, b: g.b, slot });
    }
    Ok(())
}

fn resolve_images<B: Gpu2dBackend>(
    images: &[ImageDraw],
    cache: &mut TextureCache,
    backend: &mut B,
    frame: u64,
    out: &mut Vec<ImagePlanEntry>,
) -> AureaResult<()> {
    for draw in images {
        let (iw, ih) = (draw.image.width, draw.image.height);
        if iw == 0 || ih == 0 {
            continue;
        }
        let expected = (iw as usize).saturating_mul(ih as usize).saturating_mul(4);
        if draw.image.data.len() != expected {
            return Err(AureaError::RenderingFailed);
        }
        let slot = cache.resolve(&draw.image.data, iw, ih, backend, frame)?;
        let (iwf, ihf) = (iw as f32, ih as f32);
        out.push(ImagePlanEntry {
            rect: [draw.dest.x, draw.dest.y, draw.dest.width, draw.dest.height],
            uv: [
                draw.src.x / iwf,
                draw.src.y / ihf,
                (draw.src.x + draw.src.width) / iwf,
                (draw.src.y + draw.src.height) / ihf,
            ],
            tint: [
                draw.tint.r as f32 / 255.0,
                draw.tint.g as f32 / 255.0,
                draw.tint.b as f32 / 255.0,
                draw.tint.a as f32 / 255.0,
            ],
            slot,
        });
    }
    Ok(())
}

fn resolve_texts<B: Gpu2dBackend>(
    texts: &[TextDraw],
    cache: &mut TextureCache,
    backend: &mut B,
    frame: u64,
    out: &mut Vec<TextPlanEntry>,
) -> AureaResult<()> {
    for text in texts {
        let w = text.rect.width as u32;
        let h = text.rect.height as u32;
        if w == 0 || h == 0 {
            continue;
        }
        let slot = cache.resolve(&text.mask, w, h, backend, frame)?;
        out.push(TextPlanEntry {
            rect: [text.rect.x, text.rect.y, text.rect.width, text.rect.height],
            color: [
                text.color.r as f32 / 255.0,
                text.color.g as f32 / 255.0,
                text.color.b as f32 / 255.0,
                text.color.a as f32 / 255.0,
            ],
            slot,
        });
    }
    Ok(())
}
