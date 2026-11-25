//! Images shared by window icons and the clipboard: RGBA8 pixels, PNG on every build, and Windows
//! DIB and BMP with `clipboard`.

use crate::{AureaError, AureaResult};
use std::fmt::Display;

// Windows puts clipboard images in DIB; tests check it everywhere.
#[cfg(all(feature = "clipboard", any(windows, test)))]
mod dib;
mod gif;
mod jpeg;
mod png;

use png::decode_png_pixels;

#[cfg(all(feature = "clipboard", any(windows, test)))]
pub(crate) use dib::{decode_dib, encode_dib};
pub(crate) use png::{decode_png, encode_png};

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

    /// Decodes PNG, JPEG or GIF (its first frame), told apart by their
    /// signatures, on any thread. `max_bytes` limits the input and the
    /// decoded pixels.
    pub fn decode(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        if bytes.len() > max_bytes {
            return Err(limit_error());
        }
        let mut rgba = Vec::new();
        let (width, height) = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            decode_png_pixels(bytes, max_bytes, &mut rgba)?
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            jpeg::decode_jpeg(bytes, max_bytes, &mut rgba)?
        } else if bytes.starts_with(b"GIF8") {
            gif::decode_gif(bytes, max_bytes, &mut rgba)?
        } else {
            return Err(AureaError::ElementOperationFailed);
        };
        Ok(Self { width, height, rgba })
    }

    /// Decodes PNG on any thread.
    pub fn from_png(bytes: &[u8], max_bytes: usize) -> AureaResult<Self> {
        decode_png(bytes, max_bytes)
    }

    /// Replaces pixels from PNG, reusing this image's RGBA allocation when it
    /// fits. On failure the image becomes empty and keeps its allocation for a
    /// retry. `max_bytes` limits input, decoded pixels and the PNG decoder's own
    /// budget separately, not their combined peak or an existing buffer capacity.
    pub fn decode_png_into(&mut self, bytes: &[u8], max_bytes: usize) -> AureaResult<()> {
        self.width = 0;
        self.height = 0;
        match decode_png_pixels(bytes, max_bytes, &mut self.rgba) {
            Ok((width, height)) => {
                self.width = width;
                self.height = height;
                Ok(())
            }
            Err(error) => {
                self.rgba.clear();
                Err(error)
            }
        }
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

fn invalid(_: impl Display) -> AureaError {
    AureaError::ElementOperationFailed
}

/// Native error code when a read exceeds the caller's byte budget.
pub(crate) const READ_LIMIT_EXCEEDED: i32 = -6;

pub(crate) fn limit_error() -> AureaError {
    AureaError::PlatformError(READ_LIMIT_EXCEEDED)
}

#[cfg(test)]
#[cfg(feature = "clipboard")]
mod tests {
    use super::*;
    use ::png::{BitDepth, ColorType, Encoder};
    use std::thread;

    fn png_pixels(
        width: u32,
        height: u32,
        color: ColorType,
        depth: BitDepth,
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels)
                .unwrap();
        }
        bytes
    }

    fn bmp_file(dib: &[u8], pixel_offset: usize, gap: usize) -> Vec<u8> {
        let mut bytes = vec![0; 14];
        bytes[..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(
            &u32::try_from(14 + pixel_offset + gap)
                .unwrap()
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&dib[..pixel_offset]);
        bytes.extend_from_slice(&vec![77; gap]);
        bytes.extend_from_slice(&dib[pixel_offset..]);
        let len = u32::try_from(bytes.len()).unwrap();
        bytes[2..6].copy_from_slice(&len.to_le_bytes());
        bytes
    }

    #[test]
    fn bmp_files_obey_offsets_alpha_orientation_and_file_bounds() {
        let image = sample();
        let dib = image.to_dib().unwrap();
        let file = bmp_file(&dib, 124, 7);
        assert_eq!(ClipboardImage::from_bmp(&file, 8192).unwrap(), image);
        assert_eq!(ClipboardImage::from_encoded(&file, 8192).unwrap(), image);
        let mut bottom_up = dib;
        bottom_up[8..12].copy_from_slice(&2i32.to_le_bytes());
        for x in 0..8 {
            bottom_up.swap(124 + x, 132 + x);
        }
        assert_eq!(
            ClipboardImage::from_bmp(&bmp_file(&bottom_up, 124, 0), 8192).unwrap(),
            image
        );
        assert_eq!(
            ClipboardImage::from_encoded(&image.to_png().unwrap(), 8192).unwrap(),
            image
        );
        assert!(matches!(
            ClipboardImage::from_encoded(b"\xff\xd8\xff", 8192),
            Err(AureaError::Unsupported { .. })
        ));
    }

    #[test]
    fn bmp_rejects_truncation_overlapping_offsets_and_budget_overflow() {
        let dib = sample().to_dib().unwrap();
        let file = bmp_file(&dib, 124, 7);
        for len in 0..file.len() {
            assert!(ClipboardImage::from_bmp(&file[..len], 8192).is_err());
        }
        let mut overlap = file.clone();
        overlap[10..14].copy_from_slice(&14u32.to_le_bytes());
        assert!(ClipboardImage::from_bmp(&overlap, 8192).is_err());
        assert!(ClipboardImage::from_bmp(&file, file.len() - 1).is_err());
    }

    #[test]
    fn png_expands_many_pixels_in_place_and_reuses_storage_after_failure() {
        let mut image = sample();
        image.rgba.reserve(4096);
        let storage = image.rgba.as_ptr();
        for color in [
            ColorType::Rgba,
            ColorType::Rgb,
            ColorType::Grayscale,
            ColorType::GrayscaleAlpha,
        ] {
            let mut source = Vec::new();
            let mut expected = Vec::new();
            for i in 0..17 * 9 {
                let v = u8::try_from(i * 37 % 256).unwrap();
                match color {
                    ColorType::Rgba => {
                        source.extend([v, 30, 170, v]);
                        expected.extend([v, 30, 170, v]);
                    }
                    ColorType::Rgb => {
                        source.extend([v, 30, 170]);
                        expected.extend([v, 30, 170, 255]);
                    }
                    ColorType::GrayscaleAlpha => {
                        source.extend([v, 255 - v]);
                        expected.extend([v, v, v, 255 - v]);
                    }
                    _ => {
                        source.push(v);
                        expected.extend([v, v, v, 255]);
                    }
                }
            }
            let bytes = png_pixels(17, 9, color, BitDepth::Eight, &source);
            image.decode_png_into(&bytes, 8192).unwrap();
            assert_eq!(image.rgba, expected);
            assert_eq!(
                image.rgba.as_ptr(),
                storage,
                "reuse the existing RGBA allocation"
            );
        }
        assert!(image.decode_png_into(b"broken", 8192).is_err());
        assert_eq!((image.width, image.height, image.rgba.len()), (0, 0, 0));
        let bytes = png_pixels(1, 1, ColorType::Grayscale, BitDepth::Sixteen, &[0xAB, 0xCD]);
        image.decode_png_into(&bytes, 8192).unwrap();
        assert_eq!(image.rgba, [0xAB, 0xAB, 0xAB, 255]);
        assert_eq!(image.rgba.as_ptr(), storage);
    }

    #[test]
    fn indexed_png_keeps_palette_transparency_during_expansion() {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, 4, 1);
            encoder.set_color(ColorType::Indexed);
            encoder.set_depth(BitDepth::Two);
            encoder.set_palette(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 80, 90, 100][..]);
            encoder.set_trns(&[0, 64, 128, 255][..]);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[0b00011011])
                .unwrap();
        }
        assert_eq!(
            ClipboardImage::from_png(&bytes, 8192).unwrap().rgba,
            [
                255, 0, 0, 0, 0, 255, 0, 64, 0, 0, 255, 128, 80, 90, 100, 255
            ]
        );
    }

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
        assert_eq!(
            ClipboardImage::from_bmp(&bmp_file(&dib, 40, 11), 4096)
                .unwrap()
                .rgba,
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
