//! What a backdrop does to the pixels already drawn under a shape: blur, saturate, tint, or an
//! app's own filter. Without the `effects` feature a backdrop draws nothing.

use std::fmt::Debug;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Arc;

use crate::types::Color;

/// One step a backdrop runs on what is drawn under its shape, in the order given.
///
/// ```rust,no_run
/// # use aurea_render::{Color, DrawingContext, Effect, Path, Rect};
/// # fn f(ctx: &mut dyn DrawingContext) -> aurea_foundation::AureaResult<()> {
/// let shape = Path::rounded_rect(Rect::new(20.0, 20.0, 300.0, 200.0), 24.0);
/// ctx.backdrop(&shape, &[Effect::Blur(20.0), Effect::Saturate(1.4)])?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub enum Effect {
    /// A blur reaching about this many pixels.
    Blur(f32),
    /// Colour scaled away from grey: 0 grey, 1 unchanged, above 1 richer.
    Saturate(f32),
    /// Every channel scaled: below 1 darker, above 1 lighter.
    Brightness(f32),
    /// A colour laid over, by its alpha.
    Tint(Color),
    /// The app's own filter. Backends that cannot run it skip it.
    Custom(Arc<dyn BackdropFilter>),
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Blur(a), Self::Blur(b))
            | (Self::Saturate(a), Self::Saturate(b))
            | (Self::Brightness(a), Self::Brightness(b)) => a.to_bits() == b.to_bits(),
            (Self::Tint(a), Self::Tint(b)) => a == b,
            (Self::Custom(a), Self::Custom(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Effect {
    /// How far past a pixel this step reads.
    pub fn reach(&self) -> f32 {
        match self {
            // three box passes of `half` each way, not `radius`: they read past it when rounded
            Self::Blur(radius) => 3.0 * (radius.max(0.0) / 3.0).round(),
            Self::Custom(filter) => filter.reach().max(0.0),
            _ => 0.0,
        }
    }
}

/// An app's own backdrop step, run on the CPU over the pixels under a shape.
///
/// Implement it on a type of your own and pass it as [`Effect::Custom`]; it gets every pixel of
/// the shape's box and changes them in place.
pub trait BackdropFilter: Debug + Send + Sync + UnwindSafe + RefUnwindSafe {
    /// Changes `pixels`, each straight ARGB as `0xAARRGGBB`.
    fn filter(&self, pixels: &mut BackdropPixels<'_>);

    /// How far past a pixel the filter reads, so the backdrop keeps that much around its box.
    fn reach(&self) -> f32 {
        0.0
    }
}

/// The pixels under a backdrop: a box of rows, each straight ARGB as `0xAARRGGBB`.
#[derive(Debug)]
pub struct BackdropPixels<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
}

impl<'a> BackdropPixels<'a> {
    /// `pixels` as `height` rows of `width`.
    pub fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
        debug_assert_eq!(pixels.len(), width * height);
        Self {
            pixels,
            width,
            height,
        }
    }

    /// Pixels across.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Rows.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Every pixel, row after row.
    pub fn pixels(&mut self) -> &mut [u32] {
        self.pixels
    }

    /// Row `y`.
    pub fn row(&mut self, y: usize) -> &mut [u32] {
        &mut self.pixels[y * self.width..(y + 1) * self.width]
    }
}
