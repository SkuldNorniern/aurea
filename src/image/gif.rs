//! GIF decoding: the first frame on a transparent canvas of the logical
//! screen size, with local or global colour tables, transparency and
//! interlacing.

use super::{invalid, pixel_bytes};
use crate::{AureaError, AureaResult};

fn bad() -> AureaError {
    AureaError::ElementOperationFailed
}

fn le16(bytes: &[u8], at: usize) -> AureaResult<usize> {
    Ok(u16::from_le_bytes([*bytes.get(at).ok_or_else(bad)?, *bytes.get(at + 1).ok_or_else(bad)?]) as usize)
}

/// Decodes the first frame into straight RGBA8.
pub(crate) fn decode_gif(bytes: &[u8], max_bytes: usize, rgba: &mut Vec<u8>) -> AureaResult<(u32, u32)> {
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return Err(bad());
    }
    let width = le16(bytes, 6)?;
    let height = le16(bytes, 8)?;
    let count = pixel_bytes(width as u32, height as u32, max_bytes)?;
    let flags = *bytes.get(10).ok_or_else(bad)?;
    let mut at = 13;
    let mut global: &[u8] = &[];
    if flags & 0x80 != 0 {
        let size = 3 << ((flags & 7) + 1);
        global = bytes.get(at..at + size).ok_or_else(bad)?;
        at += size;
    }
    let mut transparent: Option<u8> = None;
    loop {
        match *bytes.get(at).ok_or_else(bad)? {
            0x21 => {
                let label = *bytes.get(at + 1).ok_or_else(bad)?;
                at += 2;
                if label == 0xF9 && bytes.get(at).copied() == Some(4) {
                    let packed = *bytes.get(at + 1).ok_or_else(bad)?;
                    if packed & 1 != 0 {
                        transparent = Some(*bytes.get(at + 4).ok_or_else(bad)?);
                    }
                }
                at = skip_blocks(bytes, at)?;
            }
            0x2C => break,
            _ => return Err(bad()),
        }
    }
    let left = le16(bytes, at + 1)?;
    let top = le16(bytes, at + 3)?;
    let frame_w = le16(bytes, at + 5)?;
    let frame_h = le16(bytes, at + 7)?;
    let packed = *bytes.get(at + 9).ok_or_else(bad)?;
    at += 10;
    let mut palette = global;
    if packed & 0x80 != 0 {
        let size = 3 << ((packed & 7) + 1);
        palette = bytes.get(at..at + size).ok_or_else(bad)?;
        at += size;
    }
    let interlaced = packed & 0x40 != 0;
    let min_code = *bytes.get(at).ok_or_else(bad)? as u32;
    if !(1..=11).contains(&min_code) {
        return Err(bad());
    }
    at += 1;
    let mut data = Vec::new();
    loop {
        let size = *bytes.get(at).ok_or_else(bad)? as usize;
        at += 1;
        if size == 0 {
            break;
        }
        data.extend_from_slice(bytes.get(at..at + size).ok_or_else(bad)?);
        at += size;
    }
    let pixels = frame_w.checked_mul(frame_h).ok_or_else(bad)?;
    if pixels.saturating_mul(4) > max_bytes {
        return Err(super::limit_error());
    }
    let indices = lzw(&data, min_code, pixels)?;

    rgba.clear();
    rgba.try_reserve_exact(count).map_err(invalid)?;
    rgba.resize(count, 0);
    let rows: Vec<usize> = if interlaced {
        [(0, 8), (4, 8), (2, 4), (1, 2)].iter().flat_map(|&(start, step)| (start..frame_h).step_by(step)).collect()
    } else {
        (0..frame_h).collect()
    };
    for (source_row, &row) in rows.iter().enumerate() {
        let y = top + row;
        if y >= height {
            continue;
        }
        for column in 0..frame_w {
            let x = left + column;
            if x >= width {
                continue;
            }
            let Some(&index) = indices.get(source_row * frame_w + column) else {
                continue;
            };
            if Some(index) == transparent {
                continue;
            }
            let p = index as usize * 3;
            if let Some(colour) = palette.get(p..p + 3) {
                let out = (y * width + x) * 4;
                rgba[out..out + 3].copy_from_slice(colour);
                rgba[out + 3] = 255;
            }
        }
    }
    Ok((width as u32, height as u32))
}

fn skip_blocks(bytes: &[u8], mut at: usize) -> AureaResult<usize> {
    loop {
        let size = *bytes.get(at).ok_or_else(bad)? as usize;
        at += 1 + size;
        if size == 0 {
            return Ok(at);
        }
    }
}

/// Variable-width LZW as GIF uses it, up to `limit` indices.
fn lzw(data: &[u8], min_code: u32, limit: usize) -> AureaResult<Vec<u8>> {
    let clear = 1u32 << min_code;
    let end = clear + 1;
    // Each entry: (prefix code, last byte, length).
    let mut table: Vec<(u32, u8, u32)> = Vec::with_capacity(4096);
    let reset = |table: &mut Vec<(u32, u8, u32)>| {
        table.clear();
        for i in 0..clear {
            table.push((u32::MAX, i as u8, 1));
        }
        table.push((u32::MAX, 0, 0));
        table.push((u32::MAX, 0, 0));
    };
    reset(&mut table);
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve_exact(limit).map_err(invalid)?;
    let mut size = min_code + 1;
    let mut previous: Option<u32> = None;
    let (mut buffer, mut bits, mut at) = (0u32, 0u32, 0usize);
    let mut scratch = Vec::new();
    while out.len() < limit {
        while bits < size {
            let Some(&byte) = data.get(at) else {
                return Ok(out);
            };
            buffer |= (byte as u32) << bits;
            bits += 8;
            at += 1;
        }
        let code = buffer & ((1 << size) - 1);
        buffer >>= size;
        bits -= size;
        if code == clear {
            reset(&mut table);
            size = min_code + 1;
            previous = None;
            continue;
        }
        if code == end {
            break;
        }
        let first_of = |table: &Vec<(u32, u8, u32)>, mut c: u32| {
            while table[c as usize].0 != u32::MAX {
                c = table[c as usize].0;
            }
            table[c as usize].1
        };
        let known = (code as usize) < table.len();
        let emit = if known {
            code
        } else if let Some(p) = previous {
            if code as usize != table.len() {
                return Err(bad());
            }
            let first = first_of(&table, p);
            let length = table[p as usize].2 + 1;
            table.push((p, first, length));
            code
        } else {
            return Err(bad());
        };
        if known && let Some(p) = previous && table.len() < 4096 {
            let first = first_of(&table, code);
            let length = table[p as usize].2 + 1;
            table.push((p, first, length));
        }
        // Write the string for `emit`, back to front.
        scratch.clear();
        let mut c = emit;
        loop {
            let (prefix, byte, _) = table[c as usize];
            scratch.push(byte);
            if prefix == u32::MAX {
                break;
            }
            c = prefix;
        }
        for &byte in scratch.iter().rev() {
            if out.len() < limit {
                out.push(byte);
            }
        }
        previous = Some(code);
        if table.len() as u32 == 1 << size && size < 12 {
            size += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_by_one_gif_decodes_with_transparency() {
        // 2x1, global palette black/white, GCE making index 0 transparent,
        // pixels [1, 0]: LZW codes clear(4), 1, 0, end(5) at 3 bits.
        let mut gif = b"GIF89a\x02\x00\x01\x00\x80\x00\x00".to_vec();
        gif.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        gif.extend_from_slice(&[0x21, 0xF9, 4, 1, 0, 0, 0, 0]);
        gif.extend_from_slice(&[0x2C, 0, 0, 0, 0, 2, 0, 1, 0, 0, 2]);
        // codes 4,1,0,5 in 3-bit LSB-first: 100 001 000 101 -> bits.
        let packed: u32 = 4 | (1 << 3) | (0 << 6) | (5 << 9);
        gif.extend_from_slice(&[2, packed as u8, (packed >> 8) as u8, 0, 0x3B]);
        let mut rgba = Vec::new();
        assert_eq!(decode_gif(&gif, 1 << 20, &mut rgba).unwrap(), (2, 1));
        assert_eq!(rgba, [255, 255, 255, 255, 0, 0, 0, 0]);
    }
}
