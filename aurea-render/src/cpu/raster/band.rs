//! A run of whole rows of the frame buffer, indexed as the whole buffer is, so drawing code keeps
//! its `y * width + x` and rows apart can be drawn on threads apart.

use std::ops::{Index, IndexMut, Range, RangeFrom, RangeInclusive};

/// Rows of the frame buffer from pixel `start` on, as `buf[start..start + data.len()]` of the
/// whole one. Indexing outside them panics, as indexing past a slice does.
pub struct Band<'a> {
    data: &'a mut [u32],
    start: usize,
}

impl<'a> Band<'a> {
    /// All of `buf`.
    pub fn whole(buf: &'a mut [u32]) -> Self {
        Self {
            data: buf,
            start: 0,
        }
    }

    /// `rows`, which begin at pixel `start` of the whole buffer.
    pub fn new(rows: &'a mut [u32], start: usize) -> Self {
        Self { data: rows, start }
    }

    /// One past the last pixel, in the whole buffer's numbering.
    pub fn len(&self) -> usize {
        self.start + self.data.len()
    }

    /// The pixel or span at `at`, or `None` where it leaves the band.
    pub fn get_mut<A: At>(&mut self, at: A) -> Option<&mut A::Out> {
        at.get_mut(self)
    }

    fn local(&self, r: Range<usize>) -> Range<usize> {
        r.start - self.start..r.end - self.start
    }
}

/// What [`Band::get_mut`] takes: a pixel's index or a span of them.
pub trait At {
    /// A pixel or a slice of them.
    type Out: ?Sized;
    /// `self` in `band`, if all of it is there.
    fn get_mut<'b>(self, band: &'b mut Band<'_>) -> Option<&'b mut Self::Out>;
}

impl At for usize {
    type Out = u32;

    fn get_mut<'b>(self, band: &'b mut Band<'_>) -> Option<&'b mut u32> {
        let i = self.checked_sub(band.start)?;
        band.data.get_mut(i)
    }
}

impl At for Range<usize> {
    type Out = [u32];

    fn get_mut<'b>(self, band: &'b mut Band<'_>) -> Option<&'b mut [u32]> {
        let start = self.start.checked_sub(band.start)?;
        let end = self.end.checked_sub(band.start)?;
        band.data.get_mut(start..end)
    }
}

impl Index<usize> for Band<'_> {
    type Output = u32;

    fn index(&self, i: usize) -> &u32 {
        &self.data[i - self.start]
    }
}

impl IndexMut<usize> for Band<'_> {
    fn index_mut(&mut self, i: usize) -> &mut u32 {
        &mut self.data[i - self.start]
    }
}

impl Index<Range<usize>> for Band<'_> {
    type Output = [u32];

    fn index(&self, r: Range<usize>) -> &[u32] {
        &self.data[self.local(r)]
    }
}

impl IndexMut<Range<usize>> for Band<'_> {
    fn index_mut(&mut self, r: Range<usize>) -> &mut [u32] {
        let r = self.local(r);
        &mut self.data[r]
    }
}

impl Index<RangeFrom<usize>> for Band<'_> {
    type Output = [u32];

    fn index(&self, r: RangeFrom<usize>) -> &[u32] {
        &self.data[r.start - self.start..]
    }
}

impl IndexMut<RangeFrom<usize>> for Band<'_> {
    fn index_mut(&mut self, r: RangeFrom<usize>) -> &mut [u32] {
        &mut self.data[r.start - self.start..]
    }
}

impl Index<RangeInclusive<usize>> for Band<'_> {
    type Output = [u32];

    fn index(&self, r: RangeInclusive<usize>) -> &[u32] {
        &self[*r.start()..*r.end() + 1]
    }
}

impl IndexMut<RangeInclusive<usize>> for Band<'_> {
    fn index_mut(&mut self, r: RangeInclusive<usize>) -> &mut [u32] {
        &mut self[*r.start()..*r.end() + 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_reads_and_writes_by_whole_buffer_index() {
        let mut buf = vec![0u32; 12];
        let (_, rows) = buf.split_at_mut(4);
        let mut band = Band::new(&mut rows[..4], 4);
        band[5] = 7;
        band[6..8].fill(9);
        assert_eq!((band.len(), band.get_mut(3)), (8, None));
        assert_eq!(band.get_mut(8), None);
        assert_eq!(band.get_mut(3..5), None);
        assert_eq!(buf, [0, 0, 0, 0, 0, 7, 9, 9, 0, 0, 0, 0]);
    }
}
