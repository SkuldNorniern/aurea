//! Windows DIB and BMP images, for the clipboard's bitmap formats.

use super::{ClipboardImage, invalid, limit_error, pixel_bytes};
use crate::{AureaError, AureaResult};

impl ClipboardImage {
    /// Encodes a top-down Windows DIBV5 with explicit straight-alpha masks.
    pub fn to_dib(&self) -> AureaResult<Vec<u8>> {
        encode_dib(self)
    }

    /// Decodes supported 24/32-bit Windows DIBs on any thread.
    pub fn from_dib(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        decode_dib(bytes, max_bytes)
    }

    /// Decodes a BMP file containing a supported 24/32-bit DIB. Handles top-down
    /// and bottom-up rows, padding and the file's pixel offset. Indexed, RLE and
    /// arbitrary colour masks are unsupported, as in [`Self::from_dib`].
    pub fn from_bmp(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        if bytes.len() > max_bytes {
            return Err(limit_error());
        }
        if !bytes.starts_with(b"BM") {
            return Err(AureaError::ElementOperationFailed);
        }
        let declared = usize::try_from(word(bytes, 2)?).map_err(invalid)?;
        let end = if declared == 0 { bytes.len() } else { declared };
        let offset = usize::try_from(word(bytes, 10)?).map_err(invalid)?;
        if end < 14 || end > bytes.len() || offset < 14 || offset > end {
            return Err(AureaError::ElementOperationFailed);
        }
        decode_dib_at(&bytes[14..end], max_bytes, Some(offset - 14))
    }

    /// Detects PNG or BMP by its signature and decodes it on the calling thread.
    /// Other encodings return `Unsupported`; this does not perform colour-profile
    /// conversion, JPEG decoding or animated-image playback.
    pub fn from_encoded(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        if bytes.len() > max_bytes {
            return Err(limit_error());
        }
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Self::from_png(bytes, max_bytes)
        } else if bytes.starts_with(b"BM") {
            Self::from_bmp(bytes, max_bytes)
        } else {
            Err(AureaError::unsupported(
                "image decoder (PNG or BMP required)",
            ))
        }
    }
}

fn buffer(len: usize) -> AureaResult<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| AureaError::ElementOperationFailed)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

pub(crate) fn encode_dib(image: &ClipboardImage) -> AureaResult<Vec<u8>> {
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

pub(crate) fn decode_dib(bytes: &[u8], max_bytes: usize) -> AureaResult<ClipboardImage> {
    decode_dib_at(bytes, max_bytes, None)
}

fn decode_dib_at(
    bytes: &[u8],
    max_bytes: usize,
    pixel_offset: Option<usize>,
) -> AureaResult<ClipboardImage> {
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
    if let Some(file_offset) = pixel_offset {
        if file_offset < offset {
            return Err(AureaError::ElementOperationFailed);
        }
        offset = file_offset;
    }
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
