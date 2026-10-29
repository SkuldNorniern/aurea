//! What `CpuRasterizer::end_frame` counts and times when asked to.

use std::time::{Duration, Instant};

use crate::scene::command::DrawCommand;

/// Command kinds that [`FrameCounters`] times apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Clear,
    Rect,
    RoundRect,
    Circle,
    Path,
    /// Text and glyph masks.
    Glyphs,
    /// Image draws.
    Image,
    Gradient,
    Shadow,
    /// Textures and anything else.
    Other,
}

impl CommandKind {
    /// Number of kinds: the length of [`FrameCounters::kinds`].
    pub const COUNT: usize = Self::Other as usize + 1;

    pub(super) fn of(command: &DrawCommand) -> Self {
        match command {
            DrawCommand::Clear(_) => Self::Clear,
            DrawCommand::DrawRect(..) => Self::Rect,
            DrawCommand::DrawRoundRect(..) => Self::RoundRect,
            DrawCommand::DrawCircle(..) => Self::Circle,
            DrawCommand::DrawPath(..) => Self::Path,
            DrawCommand::DrawGlyphMask(..) => Self::Glyphs,
            DrawCommand::DrawImageRect(..)
            | DrawCommand::DrawImageRegion(..)
            | DrawCommand::DrawImageQuad(..) => Self::Image,
            DrawCommand::FillLinearGradient(..) | DrawCommand::FillRadialGradient(..) => {
                Self::Gradient
            }
            DrawCommand::DrawShadow(..) => Self::Shadow,
            DrawCommand::DrawTexture(..) | DrawCommand::DrawTextureQuad(..) => Self::Other,
        }
    }
}

/// Runs and time of one [`CommandKind`] in a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindTiming {
    /// Commands of this kind run.
    pub count: usize,
    /// Time spent running them.
    pub time: Duration,
}

/// What `CpuRasterizer::end_frame` did, when counting is on.
///
/// Every item is in exactly one of `drawn`, `hidden` or `skipped_clean`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameCounters {
    /// Items in the display list.
    pub items: usize,
    /// Items run.
    pub drawn: usize,
    /// Items not run because no tile they cover is dirty.
    pub skipped_clean: usize,
    /// Items not run because opaque draws above them cover them.
    pub hidden: usize,
    /// Dirty tiles this frame.
    pub dirty_tiles: usize,
    /// Per [`CommandKind`], indexed by `kind as usize`. Time covers the runs only.
    pub kinds: [KindTiming; CommandKind::COUNT],
    /// Wall time of the whole `end_frame` call.
    pub total: Duration,
}

impl FrameCounters {
    fn record(&mut self, kind: CommandKind, started: Option<Instant>) {
        self.drawn += 1;
        let timing = &mut self.kinds[kind as usize];
        timing.count += 1;
        if let Some(t) = started {
            timing.time += t.elapsed();
        }
    }
}

/// The frame's counters, or nothing when counting is off. Keeps the per-item branches out of
/// `render_frame`.
pub(super) struct Tally<'a>(pub(super) Option<&'a mut FrameCounters>);

impl Tally<'_> {
    /// The time a run starts at; `None` when counting is off, so no clock is read.
    pub(super) fn start(&self) -> Option<Instant> {
        self.0.is_some().then(Instant::now)
    }

    pub(super) fn set_items(&mut self, items: usize) {
        if let Some(c) = &mut self.0 {
            c.items = items;
        }
    }

    pub(super) fn set_dirty_tiles(&mut self, dirty: &[bool]) {
        if let Some(c) = &mut self.0 {
            c.dirty_tiles = dirty.iter().filter(|d| **d).count();
        }
    }

    /// An item not run: hidden if `occluded`, else clean.
    pub(super) fn skip(&mut self, occluded: bool) {
        if let Some(c) = &mut self.0 {
            if occluded {
                c.hidden += 1;
            } else {
                c.skipped_clean += 1;
            }
        }
    }

    /// A clear: run, or hidden when a later opaque draw covers it.
    pub(super) fn clear(&mut self, runs: bool, started: Option<Instant>) {
        if runs {
            self.ran(CommandKind::Clear, started);
        } else {
            self.skip(true);
        }
    }

    pub(super) fn ran(&mut self, kind: CommandKind, started: Option<Instant>) {
        if let Some(c) = &mut self.0 {
            c.record(kind, started);
        }
    }
}
