//! One display item: the command to the shape that draws it.

use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::fill::{self, PathScratch};
use crate::cpu::raster::image::ImageScratch;
use crate::cpu::raster::{circle, glyph, gradient, image, pixel, rect, round_rect};
use crate::cpu::stroke;
use crate::scene::command::DrawCommand;
use crate::scene::display_list::DisplayItem;
use crate::types::{BlendMode, Paint, PaintStyle, Path, Rect};
use aurea_foundation::AureaResult;
use std::mem;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    item: &DisplayItem,
    buf: &mut Band<'_>,
    scratch_path: &mut PathScratch,
    scratch_row: &mut ImageScratch,
    bw: u32,
    clip: ClipBox,
) -> AureaResult<()> {
    if clip.is_empty() {
        return Ok(());
    }
    let alpha = item.opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return Ok(());
    }
    match &item.command {
        DrawCommand::DrawRect(rect, paint) => {
            rect::draw(
                rect,
                &pixel::fade_paint(paint, alpha),
                item.blend_mode,
                buf,
                bw,
                clip,
            );
        }
        DrawCommand::DrawRoundRect(rect, radius, paint) => {
            round_rect::draw(
                *rect,
                *radius,
                &pixel::fade_paint(paint, alpha),
                item.blend_mode,
                buf,
                bw,
                clip,
            );
        }
        DrawCommand::DrawShadow(rect, radius, blur, color) => {
            round_rect::shadow(
                *rect,
                *radius,
                *blur,
                pixel::fade_color(*color, alpha),
                item.blend_mode,
                buf,
                bw,
                clip,
            );
        }
        DrawCommand::DrawCircle(center, radius, paint) => {
            circle::draw(
                *center,
                *radius,
                &pixel::fade_paint(paint, alpha),
                item.blend_mode,
                buf,
                bw,
                clip,
            );
        }
        DrawCommand::DrawPath(path, paint) => {
            draw_path(
                path,
                &pixel::fade_paint(paint, alpha),
                item.blend_mode,
                buf,
                scratch_path,
                bw,
                clip,
            )?;
        }
        DrawCommand::DrawGlyphMask(mask, origin, color) => {
            glyph::draw_blended(
                mask,
                *origin,
                pixel::fade_color(*color, alpha),
                item.blend_mode,
                buf,
                bw,
                clip,
            );
        }
        DrawCommand::DrawImageQuad(image, src, quad, sampling) => {
            image::draw_quad(
                image,
                *src,
                *quad,
                *sampling,
                item.blend_mode,
                alpha,
                buf,
                scratch_row,
                true,
                bw,
                clip,
            );
        }
        DrawCommand::DrawImageRect(image, dest, sampling) => {
            let src = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
            image::draw(
                image,
                src,
                *dest,
                *sampling,
                item.blend_mode,
                alpha,
                buf,
                scratch_row,
                bw,
                clip,
            );
        }
        DrawCommand::DrawImageRegion(image, src, dest, sampling) => {
            image::draw(
                image,
                *src,
                *dest,
                *sampling,
                item.blend_mode,
                alpha,
                buf,
                scratch_row,
                bw,
                clip,
            );
        }
        DrawCommand::FillLinearGradient(grad, rect) => {
            let faded;
            let grad = if alpha < 1.0 {
                faded = gradient::fade_linear(grad, alpha);
                &faded
            } else {
                grad
            };
            gradient::fill_linear(grad, *rect, item.blend_mode, buf, bw, clip);
        }
        DrawCommand::FillRadialGradient(grad, rect) => {
            let faded;
            let grad = if alpha < 1.0 {
                faded = gradient::fade_radial(grad, alpha);
                &faded
            } else {
                grad
            };
            gradient::fill_radial(grad, *rect, item.blend_mode, buf, bw, clip);
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_path(
    path: &Path,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    scratch_path: &mut PathScratch,
    bw: u32,
    clip: ClipBox,
) -> AureaResult<()> {
    if paint.style == PaintStyle::Stroke {
        let reused = mem::take(&mut scratch_path.outline);
        let outline = stroke::outline_path_in(path, &stroke::StrokeStyle::of(paint), reused);
        let drawn = fill_path(&outline, paint, mode, buf, scratch_path, bw, clip);
        scratch_path.outline = outline.commands;
        return drawn;
    }

    fill_path(path, paint, mode, buf, scratch_path, bw, clip)
}

fn fill_path(
    path: &Path,
    paint: &Paint,
    mode: BlendMode,
    buf: &mut Band<'_>,
    scratch_path: &mut PathScratch,
    bw: u32,
    clip: ClipBox,
) -> AureaResult<()> {
    fill::fill_path(path, paint.color, mode, buf, bw, clip, scratch_path);
    Ok(())
}
