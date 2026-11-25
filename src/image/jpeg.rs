//! JPEG decoding: baseline and progressive Huffman-coded frames, any chroma
//! subsampling, restart intervals, greyscale, YCbCr and Adobe CMYK/YCCK.
//! Arithmetic coding and lossless frames are not supported.

use super::{invalid, limit_error, pixel_bytes};
use crate::{AureaError, AureaResult};

/// Natural order index of each zigzag position.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
    20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

fn bad() -> AureaError {
    AureaError::ElementOperationFailed
}

/// A Huffman table as code lengths, decoded canonically.
#[derive(Clone, Default)]
struct Huffman {
    /// Fast lookup of the first 9 bits: (length, symbol), length 0 if longer.
    fast: Vec<(u8, u8)>,
    /// Per length 1..=16: largest code of that length, or -1.
    max_code: [i32; 18],
    /// Per length: index of the first symbol minus the first code.
    offset: [i32; 17],
    symbols: Vec<u8>,
}

const FAST_BITS: u32 = 9;

impl Huffman {
    fn new(counts: &[u8; 16], symbols: Vec<u8>) -> AureaResult<Self> {
        let mut table = Huffman { fast: vec![(0, 0); 1 << FAST_BITS], max_code: [-1; 18], offset: [0; 17], symbols };
        let mut code = 0i32;
        let mut k = 0usize;
        for length in 1..=16usize {
            let n = counts[length - 1] as usize;
            table.offset[length] = k as i32 - code;
            for _ in 0..n {
                if k >= table.symbols.len() {
                    return Err(bad());
                }
                if length as u32 <= FAST_BITS {
                    let shift = FAST_BITS - length as u32;
                    let start = (code as usize) << shift;
                    for slot in start..start + (1 << shift) {
                        if let Some(entry) = table.fast.get_mut(slot) {
                            *entry = (length as u8, table.symbols[k]);
                        }
                    }
                }
                code += 1;
                k += 1;
            }
            table.max_code[length] = if n > 0 { code - 1 } else { -1 };
            if code > 1 << length {
                return Err(bad());
            }
            code <<= 1;
        }
        table.max_code[17] = i32::MAX;
        Ok(table)
    }
}

/// Entropy-coded data, with `FF 00` unstuffed and markers left for the caller.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    buffer: u32,
    count: u32,
    /// A marker was reached; zeros are fed from here on.
    marker: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8], at: usize) -> Self {
        Bits { data, at, buffer: 0, count: 0, marker: false }
    }

    fn fill(&mut self) {
        while self.count <= 24 {
            let mut byte = 0u32;
            if !self.marker && self.at < self.data.len() {
                byte = self.data[self.at] as u32;
                if byte == 0xFF {
                    let next = self.data.get(self.at + 1).copied().unwrap_or(0);
                    if next == 0 {
                        self.at += 2;
                    } else {
                        self.marker = true;
                        byte = 0;
                    }
                } else {
                    self.at += 1;
                }
            }
            self.buffer |= byte << (24 - self.count);
            self.count += 8;
        }
    }

    fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.fill();
        let value = self.buffer >> (32 - n);
        self.buffer <<= n;
        self.count -= n;
        value
    }

    fn bit(&mut self) -> u32 {
        self.bits(1)
    }

    fn decode(&mut self, table: &Huffman) -> AureaResult<u8> {
        self.fill();
        let (length, symbol) = table.fast[(self.buffer >> (32 - FAST_BITS)) as usize];
        if length > 0 {
            self.buffer <<= length;
            self.count -= length as u32;
            return Ok(symbol);
        }
        let mut code = 0i32;
        for length in 1..=16usize {
            code = (code << 1) | self.bit() as i32;
            if code <= table.max_code[length] {
                let index = (code + table.offset[length]) as usize;
                return table.symbols.get(index).copied().ok_or_else(bad);
            }
        }
        Err(bad())
    }

    /// A `size`-bit value, sign-extended as JPEG does.
    fn receive(&mut self, size: u32) -> i32 {
        if size == 0 {
            return 0;
        }
        let value = self.bits(size) as i32;
        if value < 1 << (size - 1) { value - (1 << size) + 1 } else { value }
    }

    /// Skips to the next restart marker and past it.
    fn restart(&mut self) {
        self.buffer = 0;
        self.count = 0;
        self.marker = false;
        while self.at + 1 < self.data.len() {
            if self.data[self.at] == 0xFF && (0xD0..=0xD7).contains(&self.data[self.at + 1]) {
                self.at += 2;
                return;
            }
            self.at += 1;
        }
    }

    /// Where the next marker starts.
    fn end(&self) -> usize {
        let mut at = self.at;
        while at + 1 < self.data.len() && !(self.data[at] == 0xFF && self.data[at + 1] != 0 && self.data[at + 1] != 0xFF) {
            at += 1;
        }
        at
    }
}

struct Component {
    id: u8,
    h: usize,
    v: usize,
    quant: usize,
    /// Blocks per line and per column, padded to whole MCUs.
    blocks_w: usize,
    blocks_h: usize,
    coefficients: Vec<i32>,
    dc_table: usize,
    ac_table: usize,
    predictor: i32,
}

struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    components: Vec<Component>,
    h_max: usize,
    v_max: usize,
    mcus_x: usize,
    mcus_y: usize,
}

/// Decodes into straight RGBA8.
pub(crate) fn decode_jpeg(bytes: &[u8], max_bytes: usize, rgba: &mut Vec<u8>) -> AureaResult<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(bad());
    }
    let mut quant = [[1u16; 64]; 4];
    let mut dc_tables: [Huffman; 4] = Default::default();
    let mut ac_tables: [Huffman; 4] = Default::default();
    let mut frame: Option<Frame> = None;
    let mut restart_interval = 0usize;
    let mut adobe_transform: Option<u8> = None;
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = bytes[at + 1];
        if marker == 0xFF || marker == 0x00 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            at += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        if marker == 0xD9 {
            break;
        }
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        let start = at + 4;
        let end = (at + 2 + length).min(bytes.len());
        if length < 2 || start > end {
            return Err(bad());
        }
        let segment = &bytes[start..end];
        match marker {
            0xDB => {
                let mut p = 0;
                while p < segment.len() {
                    let precision = segment[p] >> 4;
                    let id = (segment[p] & 3) as usize;
                    p += 1;
                    for k in 0..64 {
                        let value = if precision == 0 {
                            let v = *segment.get(p).ok_or_else(bad)? as u16;
                            p += 1;
                            v
                        } else {
                            let v = u16::from_be_bytes([*segment.get(p).ok_or_else(bad)?, *segment.get(p + 1).ok_or_else(bad)?]);
                            p += 2;
                            v
                        };
                        quant[id][ZIGZAG[k]] = value;
                    }
                }
            }
            0xC4 => {
                let mut p = 0;
                while p + 17 <= segment.len() {
                    let class = segment[p] >> 4;
                    let id = (segment[p] & 3) as usize;
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&segment[p + 1..p + 17]);
                    let total: usize = counts.iter().map(|&c| c as usize).sum();
                    p += 17;
                    let symbols = segment.get(p..p + total).ok_or_else(bad)?.to_vec();
                    p += total;
                    let table = Huffman::new(&counts, symbols)?;
                    if class == 0 {
                        dc_tables[id] = table;
                    } else {
                        ac_tables[id] = table;
                    }
                }
            }
            0xDD => restart_interval = u16::from_be_bytes([*segment.first().ok_or_else(bad)?, *segment.get(1).ok_or_else(bad)?]) as usize,
            0xEE => {
                if segment.len() >= 12 && segment.starts_with(b"Adobe") {
                    adobe_transform = Some(segment[11]);
                }
            }
            0xC0 | 0xC1 | 0xC2 => {
                if segment.len() < 6 {
                    return Err(bad());
                }
                let height = u16::from_be_bytes([segment[1], segment[2]]) as usize;
                let width = u16::from_be_bytes([segment[3], segment[4]]) as usize;
                let count = segment[5] as usize;
                pixel_bytes(width as u32, height as u32, max_bytes)?;
                let mut components = Vec::new();
                for c in 0..count {
                    let base = 6 + c * 3;
                    let info = segment.get(base..base + 3).ok_or_else(bad)?;
                    let (h, v) = ((info[1] >> 4) as usize, (info[1] & 15) as usize);
                    if h == 0 || v == 0 || h > 4 || v > 4 {
                        return Err(bad());
                    }
                    components.push(Component { id: info[0], h, v, quant: (info[2] & 3) as usize, blocks_w: 0, blocks_h: 0, coefficients: Vec::new(), dc_table: 0, ac_table: 0, predictor: 0 });
                }
                if components.is_empty() || components.len() > 4 {
                    return Err(bad());
                }
                let h_max = components.iter().map(|c| c.h).max().unwrap_or(1);
                let v_max = components.iter().map(|c| c.v).max().unwrap_or(1);
                let mcus_x = width.div_ceil(8 * h_max);
                let mcus_y = height.div_ceil(8 * v_max);
                let mut total = 0usize;
                for component in &mut components {
                    component.blocks_w = mcus_x * component.h;
                    component.blocks_h = mcus_y * component.v;
                    total += component.blocks_w * component.blocks_h * 64 * 4;
                }
                if total > max_bytes.saturating_mul(4) {
                    return Err(limit_error());
                }
                for component in &mut components {
                    let size = component.blocks_w * component.blocks_h * 64;
                    component.coefficients.try_reserve_exact(size).map_err(invalid)?;
                    component.coefficients.resize(size, 0);
                }
                frame = Some(Frame { width, height, progressive: marker == 0xC2, components, h_max, v_max, mcus_x, mcus_y });
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err(bad()),
            0xDA => {
                let frame = frame.as_mut().ok_or_else(bad)?;
                let count = *segment.first().ok_or_else(bad)? as usize;
                let mut scan = Vec::new();
                for c in 0..count {
                    let id = *segment.get(1 + c * 2).ok_or_else(bad)?;
                    let tables = *segment.get(2 + c * 2).ok_or_else(bad)?;
                    let index = frame.components.iter().position(|k| k.id == id).ok_or_else(bad)?;
                    frame.components[index].dc_table = (tables >> 4) as usize & 3;
                    frame.components[index].ac_table = (tables & 15) as usize & 3;
                    scan.push(index);
                }
                let p = 1 + count * 2;
                let spectral_start = *segment.get(p).ok_or_else(bad)? as usize;
                let spectral_end = (*segment.get(p + 1).ok_or_else(bad)? as usize).min(63);
                let approx = *segment.get(p + 2).ok_or_else(bad)?;
                let (high, low) = ((approx >> 4) as u32, (approx & 15) as u32);
                let mut bits = Bits::new(bytes, end);
                for &index in &scan {
                    frame.components[index].predictor = 0;
                }
                let mut eob_run = 0u32;
                let params = ScanParams { start: spectral_start, end: spectral_end, high, low, progressive: frame.progressive };
                decode_scan(frame, &scan, &params, &dc_tables, &ac_tables, &mut bits, restart_interval, &mut eob_run)?;
                at = bits.end();
                continue;
            }
            _ => {}
        }
        at = end;
    }
    let frame = frame.ok_or_else(bad)?;
    output(&frame, &quant, adobe_transform, max_bytes, rgba)
}

struct ScanParams {
    start: usize,
    end: usize,
    high: u32,
    low: u32,
    progressive: bool,
}

#[allow(clippy::too_many_arguments)]
fn decode_scan(
    frame: &mut Frame,
    scan: &[usize],
    params: &ScanParams,
    dc_tables: &[Huffman; 4],
    ac_tables: &[Huffman; 4],
    bits: &mut Bits<'_>,
    restart_interval: usize,
    eob_run: &mut u32,
) -> AureaResult<()> {
    // A single-component scan covers only that component's real blocks.
    let (units_x, units_y) = if scan.len() == 1 {
        let c = &frame.components[scan[0]];
        let w = (frame.width * c.h).div_ceil(frame.h_max).div_ceil(8);
        let h = (frame.height * c.v).div_ceil(frame.v_max).div_ceil(8);
        (w, h)
    } else {
        (frame.mcus_x, frame.mcus_y)
    };
    let mut done = 0usize;
    for uy in 0..units_y {
        for ux in 0..units_x {
            if restart_interval > 0 && done > 0 && done.is_multiple_of(restart_interval) {
                bits.restart();
                *eob_run = 0;
                for &index in scan {
                    frame.components[index].predictor = 0;
                }
            }
            done += 1;
            for &index in scan {
                let component = &mut frame.components[index];
                let (blocks_h, blocks_v) = if scan.len() == 1 { (1, 1) } else { (component.h, component.v) };
                for by in 0..blocks_v {
                    for bx in 0..blocks_h {
                        let (block_x, block_y) = if scan.len() == 1 { (ux, uy) } else { (ux * component.h + bx, uy * component.v + by) };
                        let at = (block_y * component.blocks_w + block_x) * 64;
                        let dc = &dc_tables[component.dc_table];
                        let ac = &ac_tables[component.ac_table];
                        let Some(block) = component.coefficients.get_mut(at..at + 64) else {
                            return Err(bad());
                        };
                        let predictor = &mut component.predictor;
                        if !params.progressive {
                            decode_baseline(block, dc, ac, bits, predictor)?;
                        } else if params.start == 0 {
                            decode_dc(block, dc, bits, predictor, params)?;
                        } else if params.high == 0 {
                            decode_ac_first(block, ac, bits, params, eob_run)?;
                        } else {
                            decode_ac_refine(block, ac, bits, params, eob_run)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn decode_baseline(block: &mut [i32], dc: &Huffman, ac: &Huffman, bits: &mut Bits<'_>, predictor: &mut i32) -> AureaResult<()> {
    let t = bits.decode(dc)? as u32;
    *predictor += bits.receive(t);
    block[0] = *predictor;
    let mut k = 1;
    while k < 64 {
        let rs = bits.decode(ac)?;
        let (run, size) = ((rs >> 4) as usize, (rs & 15) as u32);
        if size == 0 {
            if run != 15 {
                break;
            }
            k += 16;
            continue;
        }
        k += run;
        if k > 63 {
            break;
        }
        block[ZIGZAG[k]] = bits.receive(size);
        k += 1;
    }
    Ok(())
}

fn decode_dc(block: &mut [i32], dc: &Huffman, bits: &mut Bits<'_>, predictor: &mut i32, params: &ScanParams) -> AureaResult<()> {
    if params.high == 0 {
        let t = bits.decode(dc)? as u32;
        *predictor += bits.receive(t);
        block[0] = *predictor << params.low;
    } else if bits.bit() == 1 {
        block[0] |= 1 << params.low;
    }
    Ok(())
}

fn decode_ac_first(block: &mut [i32], ac: &Huffman, bits: &mut Bits<'_>, params: &ScanParams, eob_run: &mut u32) -> AureaResult<()> {
    if *eob_run > 0 {
        *eob_run -= 1;
        return Ok(());
    }
    let mut k = params.start;
    while k <= params.end {
        let rs = bits.decode(ac)?;
        let (run, size) = ((rs >> 4) as u32, (rs & 15) as u32);
        if size == 0 {
            if run < 15 {
                *eob_run = (1 << run) + bits.bits(run) - 1;
                break;
            }
            k += 16;
            continue;
        }
        k += run as usize;
        if k > 63 {
            break;
        }
        block[ZIGZAG[k]] = bits.receive(size) * (1 << params.low);
        k += 1;
    }
    Ok(())
}

fn decode_ac_refine(block: &mut [i32], ac: &Huffman, bits: &mut Bits<'_>, params: &ScanParams, eob_run: &mut u32) -> AureaResult<()> {
    let bit = 1i32 << params.low;
    let refine = |block: &mut [i32], z: usize, bits: &mut Bits<'_>| {
        if block[z] != 0 && bits.bit() == 1 && block[z] & bit == 0 {
            block[z] += if block[z] >= 0 { bit } else { -bit };
        }
    };
    let mut k = params.start;
    if *eob_run == 0 {
        while k <= params.end {
            let rs = bits.decode(ac)?;
            let (mut run, size) = ((rs >> 4) as i32, (rs & 15) as u32);
            let mut value = 0;
            if size == 0 {
                if run < 15 {
                    *eob_run = (1 << run) + bits.bits(run as u32);
                    break;
                }
            } else {
                value = if bits.bit() == 1 { bit } else { -bit };
            }
            while k <= params.end {
                let z = ZIGZAG[k];
                if block[z] != 0 {
                    refine(block, z, bits);
                } else {
                    if run == 0 {
                        if value != 0 {
                            block[z] = value;
                        }
                        k += 1;
                        break;
                    }
                    run -= 1;
                }
                k += 1;
            }
        }
    }
    if *eob_run > 0 {
        while k <= params.end {
            refine(block, ZIGZAG[k], bits);
            k += 1;
        }
        *eob_run -= 1;
    }
    Ok(())
}

/// Inverse DCT of one dequantized block into 8x8 samples.
fn idct(input: &[i32], quant: &[u16; 64], out: &mut [u8; 64], table: &[[f32; 8]; 8]) {
    let mut values = [0f32; 64];
    for i in 0..64 {
        values[i] = input[i] as f32 * quant[i] as f32;
    }
    let mut temp = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0.0;
            for u in 0..8 {
                sum += table[x][u] * values[y * 8 + u];
            }
            temp[y * 8 + x] = sum;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut sum = 0.0;
            for v in 0..8 {
                sum += table[y][v] * temp[v * 8 + x];
            }
            out[y * 8 + x] = (sum / 4.0 + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn output(frame: &Frame, quant: &[[u16; 64]; 4], adobe: Option<u8>, max_bytes: usize, rgba: &mut Vec<u8>) -> AureaResult<(u32, u32)> {
    let (width, height) = (frame.width, frame.height);
    let count = pixel_bytes(width as u32, height as u32, max_bytes)?;
    let mut table = [[0f32; 8]; 8];
    for (x, row) in table.iter_mut().enumerate() {
        for (u, cell) in row.iter_mut().enumerate() {
            let c = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            *cell = c * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos();
        }
    }
    // Each component's samples at its own resolution.
    let mut planes: Vec<(Vec<u8>, usize)> = Vec::new();
    for component in &frame.components {
        let stride = component.blocks_w * 8;
        let mut plane = Vec::new();
        plane.try_reserve_exact(stride * component.blocks_h * 8).map_err(invalid)?;
        plane.resize(stride * component.blocks_h * 8, 0);
        let mut block = [0u8; 64];
        let q = &quant[component.quant];
        for by in 0..component.blocks_h {
            for bx in 0..component.blocks_w {
                let at = (by * component.blocks_w + bx) * 64;
                idct(&component.coefficients[at..at + 64], q, &mut block, &table);
                for y in 0..8 {
                    let row = (by * 8 + y) * stride + bx * 8;
                    plane[row..row + 8].copy_from_slice(&block[y * 8..y * 8 + 8]);
                }
            }
        }
        planes.push((plane, stride));
    }
    rgba.clear();
    rgba.try_reserve_exact(count).map_err(invalid)?;
    rgba.resize(count, 255);
    // Subsampled components are interpolated between sample centres, as
    // libjpeg's smooth upsampling does.
    let sample = |c: usize, x: usize, y: usize| -> f32 {
        let component = &frame.components[c];
        let (plane, stride) = &planes[c];
        if component.h == frame.h_max && component.v == frame.v_max {
            return plane[y * stride + x] as f32;
        }
        let rows = plane.len() / stride;
        let at = |px: f32, scale: f32, limit: usize| -> (usize, usize, f32) {
            let p = ((px + 0.5) * scale - 0.5).max(0.0);
            let lo = (p.floor() as usize).min(limit - 1);
            let hi = (lo + 1).min(limit - 1);
            (lo, hi, p - lo as f32)
        };
        let real_w = (frame.width * component.h).div_ceil(frame.h_max).max(1).min(*stride);
        let real_h = (frame.height * component.v).div_ceil(frame.v_max).max(1).min(rows);
        let (x0, x1, fx) = at(x as f32, component.h as f32 / frame.h_max as f32, real_w);
        let (y0, y1, fy) = at(y as f32, component.v as f32 / frame.v_max as f32, real_h);
        let get = |sx: usize, sy: usize| plane[sy * stride + sx] as f32;
        let top = get(x0, y0) * (1.0 - fx) + get(x1, y0) * fx;
        let bottom = get(x0, y1) * (1.0 - fx) + get(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    };
    let components = frame.components.len();
    let transform = adobe.unwrap_or(if components == 3 { 1 } else { 0 });
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = match components {
                1 | 2 => {
                    let v = sample(0, x, y);
                    (v, v, v)
                }
                3 if transform == 0 => (sample(0, x, y), sample(1, x, y), sample(2, x, y)),
                3 => ycc(sample(0, x, y), sample(1, x, y), sample(2, x, y)),
                _ => {
                    let (c, m, ye) = if transform == 2 {
                        ycc(sample(0, x, y), sample(1, x, y), sample(2, x, y))
                    } else {
                        (sample(0, x, y), sample(1, x, y), sample(2, x, y))
                    };
                    let k = sample(3, x, y);
                    // Adobe stores CMYK inverted.
                    (c * k / 255.0, m * k / 255.0, ye * k / 255.0)
                }
            };
            let at = (y * width + x) * 4;
            rgba[at] = r.round().clamp(0.0, 255.0) as u8;
            rgba[at + 1] = g.round().clamp(0.0, 255.0) as u8;
            rgba[at + 2] = b.round().clamp(0.0, 255.0) as u8;
        }
    }
    Ok((width as u32, height as u32))
}

fn ycc(y: f32, cb: f32, cr: f32) -> (f32, f32, f32) {
    let (cb, cr) = (cb - 128.0, cr - 128.0);
    (y + 1.402 * cr, y - 0.344_136 * cb - 0.714_136 * cr, y + 1.772 * cb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huffman_tables_decode_canonical_codes() {
        // Lengths: two 2-bit codes (00, 01), one 3-bit code (100).
        let mut counts = [0u8; 16];
        counts[1] = 2;
        counts[2] = 1;
        let table = Huffman::new(&counts, vec![7, 8, 9]).unwrap();
        let data = [0b0001_1000u8, 0];
        let mut bits = Bits::new(&data, 0);
        assert_eq!(bits.decode(&table).unwrap(), 7);
        assert_eq!(bits.decode(&table).unwrap(), 8);
        assert_eq!(bits.decode(&table).unwrap(), 9);
    }

    #[test]
    fn values_sign_extend() {
        let data = [0b0110_0000u8, 0];
        let mut bits = Bits::new(&data, 0);
        assert_eq!(bits.receive(2), -2);
        assert_eq!(bits.receive(2), 2);
    }

    #[test]
    fn a_flat_block_decodes_to_its_dc_level() {
        let mut block = [0u8; 64];
        let mut table = [[0f32; 8]; 8];
        for (x, row) in table.iter_mut().enumerate() {
            for (u, cell) in row.iter_mut().enumerate() {
                let c = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *cell = c * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos();
            }
        }
        let mut coefficients = [0i32; 64];
        // A DC of 80 adds 80 / 8 = 10 to the 128 level shift everywhere.
        coefficients[0] = 80;
        idct(&coefficients, &[1; 64], &mut block, &table);
        assert!(block.iter().all(|&v| v == 138), "{block:?}");
    }

    /// 16x16, progressive, 4:2:0: red rises left to right, green top to
    /// bottom, blue is high in the left half and low in the right.
    const PROGRESSIVE: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
        0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x03, 0x02, 0x02, 0x03, 0x02, 0x02, 0x03,
        0x03, 0x03, 0x03, 0x04, 0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04, 0x05, 0x0A, 0x07,
        0x07, 0x06, 0x08, 0x0C, 0x0A, 0x0C, 0x0C, 0x0B, 0x0A, 0x0B, 0x0B, 0x0D, 0x0E, 0x12, 0x10, 0x0D,
        0x0E, 0x11, 0x0E, 0x0B, 0x0B, 0x10, 0x16, 0x10, 0x11, 0x13, 0x14, 0x15, 0x15, 0x15, 0x0C, 0x0F,
        0x17, 0x18, 0x16, 0x14, 0x18, 0x12, 0x14, 0x15, 0x14, 0xFF, 0xDB, 0x00, 0x43, 0x01, 0x03, 0x04,
        0x04, 0x05, 0x04, 0x05, 0x09, 0x05, 0x05, 0x09, 0x14, 0x0D, 0x0B, 0x0D, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0xFF, 0xC2,
        0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11,
        0x01, 0xFF, 0xC4, 0x00, 0x15, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x07, 0xFF, 0xC4, 0x00, 0x14, 0x01, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA,
        0x00, 0x0C, 0x03, 0x01, 0x00, 0x02, 0x10, 0x03, 0x10, 0x00, 0x00, 0x01, 0x9A, 0xB5, 0x4A, 0x68,
        0xFF, 0xC4, 0x00, 0x16, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x05, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x01,
        0x05, 0x02, 0x52, 0x50, 0xA4, 0xA1, 0x49, 0x42, 0x92, 0x8F, 0xFF, 0xC4, 0x00, 0x18, 0x11, 0x00,
        0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x05, 0x06, 0x21, 0x31, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x03, 0x01, 0x01, 0x3F, 0x01, 0x8A, 0xBE,
        0xCB, 0x3F, 0xFF, 0xC4, 0x00, 0x1A, 0x11, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x06, 0x21, 0x33, 0x52, 0xC1, 0xFF, 0xDA,
        0x00, 0x08, 0x01, 0x02, 0x01, 0x01, 0x3F, 0x01, 0x58, 0x1C, 0x44, 0xF6, 0xC1, 0xFF, 0xC4, 0x00,
        0x15, 0x10, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x31, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x06, 0x3F, 0x02, 0x88, 0x88,
        0xFF, 0xC4, 0x00, 0x15, 0x10, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x01, 0x3F,
        0x21, 0x8B, 0x16, 0x2C, 0x5F, 0xFF, 0xDA, 0x00, 0x0C, 0x03, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x10, 0x53, 0xFF, 0xC4, 0x00, 0x15, 0x11, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA1, 0xFF, 0xDA, 0x00, 0x08, 0x01,
        0x03, 0x01, 0x01, 0x3F, 0x10, 0x9A, 0xFF, 0xC4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08,
        0x01, 0x02, 0x01, 0x01, 0x3F, 0x10, 0x43, 0xFF, 0xC4, 0x00, 0x15, 0x10, 0x01, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF1, 0xFF, 0xDA,
        0x00, 0x08, 0x01, 0x01, 0x00, 0x01, 0x3F, 0x10, 0x84, 0x84, 0x84, 0x84, 0xFF, 0xD9,
    ];

    #[test]
    fn a_progressive_subsampled_jpeg_matches_a_reference_decoder() {
        let mut rgba = Vec::new();
        assert_eq!(decode_jpeg(PROGRESSIVE, 1 << 20, &mut rgba).unwrap(), (16, 16));
        // Pixels from libjpeg's decode of the same file.
        let reference = [((0, 0), [0, 2, 193]), ((15, 0), [230, 6, 33]), ((7, 8), [117, 131, 166]), ((8, 8), [124, 123, 75]), ((15, 15), [241, 238, 47])];
        for ((x, y), expected) in reference {
            let at = (y * 16 + x) * 4;
            for c in 0..3 {
                assert!(rgba[at + c].abs_diff(expected[c]) <= 4, "pixel {x},{y}: {:?} vs {expected:?}", &rgba[at..at + 3]);
            }
            assert_eq!(rgba[at + 3], 255);
        }
    }
}
