//! PNG encoding and decoding, through the `png` crate. Decoding expands every colour type to
//! straight RGBA8.

use super::{ClipboardImage, invalid, limit_error, pixel_bytes};
use crate::{AureaError, AureaResult};
use ::png::{
    BitDepth, ColorType, Compression, Decoder, DecodingError, Encoder, Limits, Transformations,
};
use std::io::Cursor;

fn decode_error(error: DecodingError) -> AureaError {
    match error {
        DecodingError::LimitsExceeded => limit_error(),
        _ => invalid(error),
    }
}

pub(crate) fn encode_png(image: &ClipboardImage) -> AureaResult<Vec<u8>> {
    if pixel_bytes(image.width, image.height, usize::MAX)? != image.rgba.len() {
        return Err(AureaError::ElementOperationFailed);
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_compression(Compression::Fast);
        let mut writer = encoder.write_header().map_err(invalid)?;
        writer.write_image_data(&image.rgba).map_err(invalid)?;
        writer.finish().map_err(invalid)?;
    }
    Ok(bytes)
}
pub(crate) fn decode_png(bytes: &[u8], max_bytes: usize) -> AureaResult<ClipboardImage> {
    let mut image = ClipboardImage {
        width: 0,
        height: 0,
        rgba: Vec::new(),
    };
    image.decode_png_into(bytes, max_bytes)?;
    Ok(image)
}

pub(crate) fn decode_png_pixels(
    bytes: &[u8],
    max_bytes: usize,
    rgba: &mut Vec<u8>,
) -> AureaResult<(u32, u32)> {
    if bytes.len() > max_bytes {
        return Err(limit_error());
    }
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_limits(Limits { bytes: max_bytes });
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let (width, height) = (reader.info().width, reader.info().height);
    let count = pixel_bytes(width, height, max_bytes)?;
    let size = reader.output_buffer_size().ok_or_else(limit_error)?;
    if size > count {
        return Err(AureaError::ElementOperationFailed);
    }
    rgba.try_reserve_exact(count.saturating_sub(rgba.len()))
        .map_err(invalid)?;
    rgba.resize(count, 0);
    // Decode into the final allocation. Expansion proceeds backward so packed
    // source pixels are never overwritten before they have been consumed.
    let output = reader.next_frame(&mut rgba[..size]).map_err(decode_error)?;
    let (width, height) = (output.width, output.height);
    let count = pixel_bytes(width, height, max_bytes)?;
    rgba.truncate(count);
    if output.bit_depth != BitDepth::Eight {
        return Err(AureaError::ElementOperationFailed);
    }
    let channels = match output.color_type {
        ColorType::Rgba => 4,
        ColorType::Rgb => 3,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Grayscale => 1,
        _ => return Err(AureaError::ElementOperationFailed),
    };
    if output.buffer_size() != count / 4 * channels {
        return Err(AureaError::ElementOperationFailed);
    }
    if output.color_type == ColorType::Rgba {
        return Ok((width, height));
    }
    for pixel in (0..count / 4).rev() {
        let at = pixel * channels;
        let colour = match channels {
            3 => [rgba[at], rgba[at + 1], rgba[at + 2], 255],
            2 => [rgba[at], rgba[at], rgba[at], rgba[at + 1]],
            _ => [rgba[at], rgba[at], rgba[at], 255],
        };
        rgba[pixel * 4..pixel * 4 + 4].copy_from_slice(&colour);
    }
    Ok((width, height))
}
