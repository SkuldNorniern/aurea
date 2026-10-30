//! Alpha-aware area filtering shared by worker previews and CPU mip fallback.

use aurea_foundation::{AureaError, AureaResult};

use crate::numeric::{f32_to_u8_clamped, f32_to_u32_clamped};

pub(super) fn validate(
    src: &[u8],
    (sw, sh): (u32, u32),
    (dw, dh): (u32, u32),
) -> AureaResult<usize> {
    let input = (sw as usize)
        .checked_mul(sh as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(AureaError::RenderingFailed)?;
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || dw > sw || dh > sh || src.len() < input {
        return Err(AureaError::RenderingFailed);
    }
    (dw as usize)
        .checked_mul(dh as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(AureaError::RenderingFailed)
}

fn reserved<T>(capacity: usize) -> AureaResult<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| AureaError::RenderingFailed)?;
    Ok(out)
}

struct Weights {
    offsets: Vec<usize>,
    taps: Vec<(usize, f32)>,
}

impl Weights {
    fn new(src: u32, dst: u32) -> AureaResult<Self> {
        if src == dst {
            return Ok(Self {
                offsets: Vec::new(),
                taps: Vec::new(),
            });
        }
        let ratio = src as f32 / dst as f32;
        let span = |i: u32| {
            let (lo, hi) = (i as f32 * ratio, (i + 1) as f32 * ratio);
            (
                f32_to_u32_clamped(lo.floor()),
                f32_to_u32_clamped(hi.ceil()).min(src),
                lo,
                hi,
            )
        };
        let mut offsets = reserved(
            (dst as usize)
                .checked_add(1)
                .ok_or(AureaError::RenderingFailed)?,
        )?;
        let mut total = 0usize;
        offsets.push(0);
        for i in 0..dst {
            let (first, last, _, _) = span(i);
            total = total
                .checked_add(last.saturating_sub(first) as usize)
                .ok_or(AureaError::RenderingFailed)?;
            offsets.push(total);
        }
        // One tightly sized tap array replaces an allocation for every column
        // or row. Keep the original float arithmetic and accumulation order.
        let mut taps = reserved(total)?;
        for i in 0..dst {
            let (first, last, lo, hi) = span(i);
            taps.extend((first..last).map(|t| {
                let weight = hi.min((t + 1) as f32) - lo.max(t as f32);
                (t as usize, weight.max(0.0) / ratio)
            }));
        }
        Ok(Self { offsets, taps })
    }

    fn at<'a>(&'a self, index: usize, identity: &'a [(usize, f32); 1]) -> &'a [(usize, f32)] {
        if self.offsets.is_empty() {
            identity
        } else {
            &self.taps[self.offsets[index]..self.offsets[index + 1]]
        }
    }
}

pub(crate) fn area_downsample(
    src: &[u8],
    (sw, sh): (u32, u32),
    (dw, dh): (u32, u32),
) -> AureaResult<Vec<u8>> {
    let count = validate(src, (sw, sh), (dw, dh))?;
    let (xs, ys) = (Weights::new(sw, dw)?, Weights::new(sh, dh)?);
    let mut out = reserved(count)?;
    for y in 0..dh as usize {
        let identity_row = [(y, 1.0)];
        let row = ys.at(y, &identity_row);
        for x in 0..dw as usize {
            let identity_column = [(x, 1.0)];
            let column = xs.at(x, &identity_column);
            let mut sum = [0.0f32; 4];
            for &(y, wy) in row {
                for &(x, wx) in column {
                    let at = (y * sw as usize + x) * 4;
                    let p = &src[at..at + 4];
                    let a = f32::from(p[3]) * wx * wy;
                    sum[0] += f32::from(p[0]) * a;
                    sum[1] += f32::from(p[1]) * a;
                    sum[2] += f32::from(p[2]) * a;
                    sum[3] += a;
                }
            }
            let alpha = sum[3];
            let straight = |c: f32| {
                if alpha > 0.0 {
                    f32_to_u8_clamped((c / alpha).round())
                } else {
                    0
                }
            };
            out.extend_from_slice(&[
                straight(sum[0]),
                straight(sum[1]),
                straight(sum[2]),
                f32_to_u8_clamped(alpha.round()),
            ]);
        }
    }
    Ok(out)
}
