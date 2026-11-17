//! Without `effects`: a backdrop draws nothing, and no frame grows over one.

use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::effect::Effect;
use crate::scene::display_list::DisplayItem;
use crate::types::Rect;

/// Buffers a backdrop reuses from one frame to the next. None without effects.
#[derive(Default)]
pub(crate) struct EffectScratch;

pub(crate) fn backdrop(
    _effects: &[Effect],
    _bounds: Rect,
    _buf: &mut Band<'_>,
    _size: (u32, u32),
    _clip: ClipBox,
    _scratch: &mut EffectScratch,
) {
}

pub(crate) fn grow_over(_items: &[DisplayItem], damage: Rect) -> Rect {
    damage
}

pub(crate) fn next_backdrop(_items: &[DisplayItem], _from: usize) -> Option<usize> {
    None
}
