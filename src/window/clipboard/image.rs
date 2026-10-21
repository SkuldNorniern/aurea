use super::{AureaError, AureaResult, limit_error};
use png::{
    BitDepth, ColorType, Compression, Decoder, DecodingError, Encoder, Limits, Transformations,
};
use std::fmt::Display;
use std::io::Cursor;

/// Tightly packed RGBA8 pixels in top-to-bottom rows, with straight alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel data, four bytes per pixel.
    pub rgba: Vec<u8>,
}

impl ClipboardImage {
    /// Encodes straight RGBA8 as PNG.
    pub fn to_png(&self) -> AureaResult<Vec<u8>> {
        encode_png(self)
    }

    /// Decodes PNG on any thread.
    pub fn from_png(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        decode_png(bytes, max_bytes)
    }

    /// Encodes a top-down Windows DIBV5 with explicit straight-alpha masks.
    pub fn to_dib(&self) -> AureaResult<Vec<u8>> {
        encode_dib(self)
    }

    /// Decodes supported 24/32-bit Windows DIBs on any thread.
    pub fn from_dib(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        decode_dib(bytes, max_bytes)
    }
}

fn pixel_bytes(width: u32, height: u32, max_bytes: usize) -> AureaResult<usize> {
    if width == 0 || height == 0 {
        return Err(AureaError::ElementOperationFailed);
    }
    let bytes = usize::try_from(width)
        .ok()
        .and_then(|width| usize::try_from(height).ok()?.checked_mul(width))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(limit_error)?;
    if bytes > max_bytes || bytes > isize::MAX as usize {
        return Err(limit_error());
    }
    Ok(bytes)
}
fn buffer(len: usize) -> AureaResult<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| AureaError::ElementOperationFailed)?;
    bytes.resize(len, 0);
    Ok(bytes)
}
fn invalid(_: impl Display) -> AureaError {
    AureaError::ElementOperationFailed
}

fn decode_error(error: DecodingError) -> AureaError {
    match error {
        DecodingError::LimitsExceeded => limit_error(),
        _ => invalid(error),
    }
}

pub(super) fn encode_png(image: &ClipboardImage) -> AureaResult<Vec<u8>> {
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
pub(super) fn decode_png(bytes: &[u8], max_bytes: usize) -> AureaResult<ClipboardImage> {
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
    let mut decoded = buffer(size)?;
    let output = reader.next_frame(&mut decoded).map_err(decode_error)?;
    let (width, height) = (output.width, output.height);
    let count = pixel_bytes(width, height, max_bytes)?;
    decoded.truncate(output.buffer_size());
    if output.color_type == ColorType::Rgba {
        if decoded.len() != count {
            return Err(AureaError::ElementOperationFailed);
        }
        return Ok(ClipboardImage {
            width,
            height,
            rgba: decoded,
        });
    }
    let mut rgba = buffer(count)?;
    match output.color_type {
        ColorType::Rgba => return Err(AureaError::ElementOperationFailed),
        ColorType::Rgb => {
            for (rgb, rgba) in decoded
                .as_chunks::<3>()
                .0
                .iter()
                .zip(rgba.as_chunks_mut::<4>().0.iter_mut())
            {
                rgba.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        ColorType::Grayscale => {
            for (&gray, rgba) in decoded.iter().zip(rgba.as_chunks_mut::<4>().0.iter_mut()) {
                rgba.copy_from_slice(&[gray, gray, gray, 255]);
            }
        }
        ColorType::GrayscaleAlpha => {
            for (gray, rgba) in decoded
                .as_chunks::<2>()
                .0
                .iter()
                .zip(rgba.as_chunks_mut::<4>().0.iter_mut())
            {
                rgba.copy_from_slice(&[gray[0], gray[0], gray[0], gray[1]]);
            }
        }
        _ => return Err(AureaError::ElementOperationFailed),
    }
    Ok(ClipboardImage {
        width,
        height,
        rgba,
    })
}

pub(super) fn encode_dib(image: &ClipboardImage) -> AureaResult<Vec<u8>> {
    let count = pixel_bytes(image.width, image.height, usize::MAX)?;
    if count != image.rgba.len()
        || image.width > i32::MAX as u32
        || image.height > i32::MAX as u32
        || count > u32::MAX as usize
    {
        return Err(AureaError::ElementOperationFailed);
    }
    let mut bytes = buffer(count.checked_add(124).ok_or_else(limit_error)?)?;
    bytes[0..4].copy_from_slice(&124u32.to_le_bytes());
    bytes[4..8].copy_from_slice(&image.width.to_le_bytes());
    let height = i32::try_from(image.height).map_err(invalid)?;
    bytes[8..12].copy_from_slice(&(-height).to_le_bytes());
    bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&32u16.to_le_bytes());
    bytes[16..20].copy_from_slice(&3u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&u32::try_from(count).map_err(invalid)?.to_le_bytes());
    for (offset, mask) in [
        (40, 0x00ff_0000u32),
        (44, 0x0000_ff00),
        (48, 0x0000_00ff),
        (52, 0xff00_0000),
        (56, 0x7352_4742),
        (108, 4),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
    }
    for (rgba, bgra) in image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bytes[124..].as_chunks_mut::<4>().0.iter_mut())
    {
        bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    Ok(bytes)
}

fn word(bytes: &[u8], at: usize) -> AureaResult<u32> {
    bytes
        .get(at..at + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(AureaError::ElementOperationFailed)
}
fn dib_masks(
    bytes: &[u8],
    header: usize,
    bits: u16,
    compression: u32,
) -> AureaResult<(usize, bool)> {
    if compression == 0 {
        return Ok((header, false));
    }
    if bits != 32
        || (header != 40 && header < 52)
        || word(bytes, 40)? != 0x00ff_0000
        || word(bytes, 44)? != 0x0000_ff00
        || word(bytes, 48)? != 0x0000_00ff
    {
        return Err(AureaError::ElementOperationFailed);
    }
    let offset = if header == 40 {
        header + if compression == 6 { 16 } else { 12 }
    } else {
        header
    };
    let alpha = if header >= 56 || compression == 6 {
        word(bytes, 52)?
    } else {
        0
    };
    if !matches!(alpha, 0 | 0xff00_0000) {
        return Err(AureaError::ElementOperationFailed);
    }
    Ok((offset, alpha != 0))
}

pub(super) fn decode_dib(bytes: &[u8], max_bytes: usize) -> AureaResult<ClipboardImage> {
    if bytes.len() > max_bytes {
        return Err(limit_error());
    }
    let header = usize::try_from(word(bytes, 0)?).map_err(invalid)?;
    if header < 40 || header > bytes.len() || word(bytes, 4)? > i32::MAX as u32 {
        return Err(AureaError::ElementOperationFailed);
    }
    let width = word(bytes, 4)?;
    let signed_height = i32::from_le_bytes(word(bytes, 8)?.to_le_bytes());
    let height = signed_height.unsigned_abs();
    let count = pixel_bytes(width, height, max_bytes)?;
    let planes = u16::from_le_bytes([bytes[12], bytes[13]]);
    let bits = u16::from_le_bytes([bytes[14], bytes[15]]);
    let compression = word(bytes, 16)?;
    if planes != 1 || !matches!(bits, 24 | 32) || !matches!(compression, 0 | 3 | 6) {
        return Err(AureaError::ElementOperationFailed);
    }
    let (mut offset, alpha) = dib_masks(bytes, header, bits, compression)?;
    offset = offset
        .checked_add(
            usize::try_from(word(bytes, 32)?)
                .map_err(invalid)?
                .checked_mul(4)
                .ok_or_else(limit_error)?,
        )
        .ok_or_else(limit_error)?;
    let width = usize::try_from(width).map_err(invalid)?;
    let height_usize = usize::try_from(height).map_err(invalid)?;
    let row = width
        .checked_mul(usize::from(bits / 8))
        .and_then(|n| n.checked_add(3))
        .map(|n| n & !3)
        .ok_or_else(limit_error)?;
    let total = row
        .checked_mul(height_usize)
        .and_then(|n| n.checked_add(offset))
        .ok_or_else(limit_error)?;
    if total > bytes.len() {
        return Err(AureaError::ElementOperationFailed);
    }
    let mut rgba = buffer(count)?;
    for (y, dst) in rgba.chunks_exact_mut(width * 4).enumerate() {
        let source_y = if signed_height < 0 {
            y
        } else {
            height_usize - 1 - y
        };
        let src = &bytes[offset + source_y * row..offset + (source_y + 1) * row];
        for (bgr, rgba) in src
            .chunks_exact(usize::from(bits / 8))
            .take(width)
            .zip(dst.as_chunks_mut::<4>().0.iter_mut())
        {
            rgba.copy_from_slice(&[bgr[2], bgr[1], bgr[0], if alpha { bgr[3] } else { 255 }]);
        }
    }
    Ok(ClipboardImage {
        width: u32::try_from(width).map_err(invalid)?,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn public_codecs_run_on_a_worker_and_obey_read_budgets() {
        thread::spawn(|| {
            let image = sample();
            let png = image.to_png().expect("PNG");
            let dib = image.to_dib().expect("DIB");
            assert_eq!(ClipboardImage::from_png(&png, 4096).unwrap(), image);
            assert_eq!(ClipboardImage::from_dib(&dib, 4096).unwrap(), image);
            assert!(ClipboardImage::from_png(&png, png.len() - 1).is_err());
            assert!(ClipboardImage::from_dib(&dib, dib.len() - 1).is_err());
            let large = ClipboardImage {
                width: 64,
                height: 64,
                rgba: vec![0; 64 * 64 * 4],
            };
            let png = large.to_png().unwrap();
            assert!(ClipboardImage::from_png(&png, 1024).is_err());
        })
        .join()
        .expect("worker");
    }

    fn sample() -> ClipboardImage {
        ClipboardImage {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 37, 49, 61, 1],
        }
    }

    #[test]
    fn png_and_dib_preserve_orientation_alpha_and_transparent_rgb() {
        let image = sample();
        assert_eq!(
            decode_png(&encode_png(&image).expect("PNG"), 4096).expect("decode"),
            image
        );
        let mut dib = encode_dib(&image).expect("DIB");
        assert_eq!(decode_dib(&dib, 4096).expect("decode"), image);
        dib[8..12].copy_from_slice(&2i32.to_le_bytes());
        for x in 0..8 {
            dib.swap(124 + x, 132 + x);
        }
        assert_eq!(decode_dib(&dib, 4096).expect("bottom-up"), image);
    }

    #[test]
    fn rgb_and_gray_png_expand_to_straight_rgba() {
        for (color, input, expected) in [
            (ColorType::Rgb, vec![12, 34, 56], vec![12, 34, 56, 255]),
            (
                ColorType::GrayscaleAlpha,
                vec![12, 34],
                vec![12, 12, 12, 34],
            ),
            (ColorType::Grayscale, vec![12], vec![12, 12, 12, 255]),
        ] {
            let mut bytes = Vec::new();
            {
                let mut encoder = Encoder::new(&mut bytes, 1, 1);
                encoder.set_color(color);
                encoder.set_depth(BitDepth::Eight);
                encoder
                    .write_header()
                    .expect("header")
                    .write_image_data(&input)
                    .expect("pixels");
            }
            assert_eq!(decode_png(&bytes, 4096).expect("decode").rgba, expected);
        }
    }

    #[test]
    fn dib_rows_obey_padding_and_unmarked_rgb_is_opaque() {
        let mut dib = vec![0; 48];
        dib[..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1u32.to_le_bytes());
        dib[8..12].copy_from_slice(&2i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib[40..].copy_from_slice(&[255, 0, 0, 77, 0, 255, 0, 88]);
        assert_eq!(
            decode_dib(&dib, 4096).expect("24-bit").rgba,
            [0, 255, 0, 255, 0, 0, 255, 255]
        );
    }

    #[test]
    fn malformed_and_oversized_images_fail_before_pixel_allocation() {
        let image = sample();
        let mut dib = encode_dib(&image).expect("DIB");
        for len in 0..dib.len() {
            assert!(decode_dib(&dib[..len], 4096).is_err());
        }
        assert!(matches!(
            decode_dib(&dib, 15),
            Err(AureaError::PlatformError(-6))
        ));
        dib[4..8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(
            decode_dib(&dib, 4096),
            Err(AureaError::PlatformError(-6))
        ));
    }

    #[test]
    fn invalid_png_and_source_pixels_are_rejected() {
        let image = sample();
        assert!(decode_png(b"not an image", 4096).is_err());
        assert!(matches!(
            decode_png(&encode_png(&image).expect("PNG"), 1),
            Err(AureaError::PlatformError(-6))
        ));
        assert!(
            encode_png(&ClipboardImage {
                width: 2,
                height: 2,
                rgba: vec![0; 3]
            })
            .is_err()
        );
    }
}
