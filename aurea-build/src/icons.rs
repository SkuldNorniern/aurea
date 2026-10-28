//! Icon files for each platform, made from one square master PNG.

use png::{BitDepth, ColorType, Decoder, Encoder, EncodingError, Transformations};
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::Path;

/// A picture as straight-alpha RGBA: `width * height * 4` bytes, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Pixels per row.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Red, green, blue and alpha of each pixel.
    pub rgba: Vec<u8>,
}

const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];
const ICNS_TYPES: [(&[u8; 4], u32); 7] = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic09", 512),
    (b"ic10", 1024),
];
const ANDROID: [(&str, u32); 5] = [
    ("mdpi", 48),
    ("hdpi", 72),
    ("xhdpi", 96),
    ("xxhdpi", 144),
    ("xxxhdpi", 192),
];
const CONTENTS_JSON: &str = r#"{
  "images" : [
    {
      "filename" : "AppIcon.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
"#;

/// The master icon at `path`, or at `path` with a `.png` extension if there is no file at `path`.
/// Errors unless the picture is square.
pub fn load_master(path: &Path) -> io::Result<Image> {
    let file = match File::open(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            File::open(path.with_extension("png"))?
        }
        opened => opened?,
    };
    let mut decoder = Decoder::new(BufReader::new(file));
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(invalid)?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| invalid("image too large"))?
    ];
    let info = reader.next_frame(&mut buf).map_err(invalid)?;
    if info.width != info.height {
        return Err(invalid("master icon is not square"));
    }
    let rgba = match info.color_type {
        ColorType::Rgba => buf,
        ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => return Err(invalid("unsupported master icon colors")),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// `image` box-filtered to `size` by `size` pixels, averaged in premultiplied alpha. Enlarging
/// repeats pixels at the edges of the source pixels they cover.
pub fn resize(image: &Image, size: u32) -> Image {
    let dst = u64::from(size);
    if image.width == 0 || image.height == 0 {
        return Image {
            width: size,
            height: size,
            rgba: vec![0; to_usize(dst * dst * 4)],
        };
    }
    let (sw, sh) = (u64::from(image.width), u64::from(image.height));
    let total = sw * sh;
    let stride = to_usize(sw) * 4;
    let xs = spans(sw, dst);
    let mut rgba = Vec::with_capacity(to_usize(dst * dst * 4));
    for rows in spans(sh, dst) {
        for cols in &xs {
            let mut color = [0u64; 3];
            let mut alpha = 0u64;
            for (sy, wy) in &rows {
                for (sx, wx) in cols {
                    let weight = wx * wy;
                    let index = sy * stride + sx * 4;
                    let p = &image.rgba[index..index + 4];
                    let a = u64::from(p[3]) * weight;
                    alpha += a;
                    for (acc, &c) in color.iter_mut().zip(p) {
                        *acc += u64::from(c) * a;
                    }
                }
            }
            rgba.extend(color.map(|c| unmultiply(c, alpha)));
            rgba.push(u8_of((alpha + total / 2) / total));
        }
    }
    Image {
        width: size,
        height: size,
        rgba,
    }
}

/// `image` as a PNG file. Empty if `rgba` does not hold `width * height` pixels.
pub fn png(image: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    match encode(image, &mut out) {
        Ok(()) => out,
        Err(_) => Vec::new(),
    }
}

/// `image` as a Windows `.ico`, with a PNG entry for each of 16 to 256 pixels.
pub fn ico(image: &Image) -> Vec<u8> {
    let entries: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .map(|&size| (size, png(&resize(image, size))))
        .collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend(u16_of(entries.len()).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in &entries {
        let dim = if *size == 256 {
            0
        } else {
            u8_of(u64::from(*size))
        };
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend(u32_of(data.len()).to_le_bytes());
        out.extend(u32_of(offset).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in entries {
        out.extend(data);
    }
    out
}

/// `image` as a macOS `.icns`, with a PNG entry for each of 16 to 1024 pixels.
pub fn icns(image: &Image) -> Vec<u8> {
    let mut body = Vec::new();
    for (kind, size) in ICNS_TYPES {
        let data = png(&resize(image, size));
        body.extend_from_slice(kind);
        body.extend(u32_of(data.len() + 8).to_be_bytes());
        body.extend(data);
    }
    let mut out = b"icns".to_vec();
    out.extend(u32_of(body.len() + 8).to_be_bytes());
    out.extend(body);
    out
}

/// Writes `image` as `mipmap-*/ic_launcher.png` in the Android resource folder `res_dir`.
pub fn write_android(image: &Image, res_dir: &Path) -> io::Result<()> {
    for (density, size) in ANDROID {
        let dir = res_dir.join(format!("mipmap-{density}"));
        fs::create_dir_all(&dir)?;
        write_if_changed(&dir.join("ic_launcher.png"), &png(&resize(image, size)))?;
    }
    Ok(())
}

/// Writes `image` at 1024 pixels as `AppIcon.png`, with its `Contents.json`, in `appiconset_dir`.
pub fn write_ios(image: &Image, appiconset_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(appiconset_dir)?;
    write_if_changed(
        &appiconset_dir.join("AppIcon.png"),
        &png(&resize(image, 1024)),
    )?;
    write_if_changed(
        &appiconset_dir.join("Contents.json"),
        CONTENTS_JSON.as_bytes(),
    )
}

/// Writes `bytes` to `path` unless the file already holds them, so build tools see no change.
fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    fs::write(path, bytes)
}

fn encode(image: &Image, out: &mut Vec<u8>) -> Result<(), EncodingError> {
    let mut encoder = Encoder::new(out, image.width, image.height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.rgba)
}

fn invalid<E: Into<Box<dyn Error + Send + Sync>>>(error: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

/// For each of `dst` output pixels, the source pixels it covers and how much of each, in units of
/// 1/`dst` source pixel, so the overlaps of one output pixel sum to `src`.
fn spans(src: u64, dst: u64) -> Vec<Vec<(usize, u64)>> {
    (0..dst)
        .map(|i| {
            let (start, end) = (i * src, (i + 1) * src);
            (start / dst..=end.saturating_sub(1) / dst)
                .map(|j| {
                    let overlap = end.min((j + 1) * dst) - start.max(j * dst);
                    (to_usize(j), overlap)
                })
                .collect()
        })
        .collect()
}

/// A channel of straight color from its alpha-weighted sum.
fn unmultiply(sum: u64, alpha: u64) -> u8 {
    (sum + alpha / 2).checked_div(alpha).map_or(0, u8_of)
}

fn to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn u8_of(value: u64) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

fn u16_of(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::io::Cursor;
    use std::process;

    fn solid(size: u32, pixel: [u8; 4]) -> Image {
        Image {
            width: size,
            height: size,
            rgba: pixel.repeat(to_usize(u64::from(size * size))),
        }
    }

    fn decode(data: &[u8]) -> Option<Image> {
        let mut reader = Decoder::new(Cursor::new(data)).read_info().ok()?;
        let mut rgba = vec![0; reader.output_buffer_size()?];
        reader.next_frame(&mut rgba).ok()?;
        let info = reader.info();
        Some(Image {
            width: info.width,
            height: info.height,
            rgba,
        })
    }

    fn be32(b: &[u8]) -> usize {
        to_usize(u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    }

    fn le32(b: &[u8]) -> usize {
        to_usize(u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
    }

    #[test]
    fn resize_keeps_a_solid_color() {
        let pixel = [10, 20, 30, 128];
        let down = resize(&solid(64, pixel), 16);
        let up = resize(&solid(64, pixel), 100);
        assert_eq!((down.width, down.height), (16, 16));
        assert!(down.rgba.as_chunks::<4>().0.iter().all(|p| *p == pixel));
        assert!(up.rgba.as_chunks::<4>().0.iter().all(|p| *p == pixel));
    }

    #[test]
    fn ico_has_a_png_entry_per_size() {
        let data = ico(&solid(64, [1, 2, 3, 255]));
        assert_eq!(&data[..4], &[0, 0, 1, 0]);
        let count = usize::from(u16::from_le_bytes([data[4], data[5]]));
        assert_eq!(count, ICO_SIZES.len());
        for (i, &size) in ICO_SIZES.iter().enumerate() {
            let entry = &data[6 + 16 * i..][..16];
            let dim = if entry[0] == 0 {
                256
            } else {
                u32::from(entry[0])
            };
            assert_eq!(dim, size);
            let (len, offset) = (le32(&entry[8..]), le32(&entry[12..]));
            let decoded = decode(&data[offset..offset + len]);
            assert_eq!(decoded.map(|i| i.width), Some(size));
        }
    }

    #[test]
    fn icns_length_matches_the_bytes() {
        let data = icns(&solid(64, [1, 2, 3, 255]));
        assert_eq!(&data[..4], b"icns");
        assert_eq!(be32(&data[4..]), data.len());
        let mut at = 8;
        for (kind, size) in ICNS_TYPES {
            assert_eq!(&data[at..at + 4], kind);
            let len = be32(&data[at + 4..]);
            let decoded = decode(&data[at + 8..at + len]);
            assert_eq!(decoded.map(|i| i.width), Some(size));
            at += len;
        }
        assert_eq!(at, data.len());
    }

    #[test]
    fn png_roundtrips_size_and_pixels() {
        let image = solid(64, [9, 8, 7, 200]);
        assert_eq!(decode(&png(&image)), Some(image));
    }

    #[test]
    fn master_loads_without_extension_and_rejects_rectangles() {
        let dir = env::temp_dir().join(format!("aurea-icons-{}", process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        fs::write(dir.join("square.png"), png(&solid(64, [5, 5, 5, 255]))).expect("write");
        let wide = Image {
            width: 64,
            height: 32,
            rgba: vec![0; 64 * 32 * 4],
        };
        fs::write(dir.join("wide.png"), png(&wide)).expect("write");
        let loaded = load_master(&dir.join("square")).map(|i| (i.width, i.height));
        assert_eq!(loaded.ok(), Some((64, 64)));
        assert!(load_master(&dir.join("wide")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
