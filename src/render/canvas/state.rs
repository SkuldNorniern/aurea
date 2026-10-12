//! What a canvas keeps between frames, and its identity.

use aurea_render::{Color, Rect};
use aurea_runtime::DamageRegion;

use super::DrawCallback;

pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame.
    pub prev_frame_damage: Option<Rect>,
}

/// A canvas's identity, for as long as the process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(pub(super) u64);

impl CanvasId {
    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}
