//! The smallest pieces: things that only draw.

mod color;
mod decor;
mod editor;
mod graph;

pub use color::{AlphaStrip, Checker, HueStrip, PickRing, SvField};
pub use decor::{Badge, BadgeTone, Separator, Spinner};
pub use editor::{DetailRow, PromptLine};
pub use graph::{AxisMarks, AxisSide, AxisStyle, GridLines, GridStyle, Marker, Stroke, Trace};

use crate::AureaResult;
use crate::render::{Color, Font, Icon, Image, Point, Rect, VectorImage};
use crate::ui::frame::{Align, Ui};
use crate::ui::state::WidgetId;

/// Something that draws itself into a rect.
pub trait Component {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()>;
}

/// A shadow under a surface: the same shape, offset and grown, in one colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub dx: f32,
    pub dy: f32,
    /// How much bigger than the surface on each side.
    pub grow: f32,
}

impl Shadow {
    /// One pixel straight down.
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            dx: 0.0,
            dy: 1.0,
            grow: 0.0,
        }
    }

    pub const fn offset(mut self, dx: f32, dy: f32) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    pub const fn grow(mut self, grow: f32) -> Self {
        self.grow = grow;
        self
    }
}

/// A line just inside a surface's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// A rounded box: shadow, fill and border, drawn in that order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub fill: Color,
    /// Corner radius; half the shorter side or more makes a pill.
    pub radius: f32,
    pub border: Option<Border>,
    pub shadow: Option<Shadow>,
}

impl Surface {
    pub const fn new(fill: Color) -> Self {
        Self {
            fill,
            radius: 0.0,
            border: None,
            shadow: None,
        }
    }

    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// A pill: the radius is clamped to half the height when drawn.
    pub const fn pill(self) -> Self {
        self.radius(f32::MAX)
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Some(Border { width, color });
        self
    }

    pub const fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Everything faded by `alpha`, for a disabled or appearing surface.
    pub fn fade(mut self, alpha: f32) -> Self {
        self.fill = self.fill.fade(alpha);
        if let Some(border) = &mut self.border {
            border.color = border.color.fade(alpha);
        }
        if let Some(shadow) = &mut self.shadow {
            shadow.color = shadow.color.fade(alpha);
        }
        self
    }
}

impl Component for Surface {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let radius = self.radius.min(rect.width / 2.0).min(rect.height / 2.0);
        if let Some(s) = self.shadow {
            let under = Rect::new(rect.x + s.dx, rect.y + s.dy, rect.width, rect.height);
            ui.round(under.inset(-s.grow), radius + s.grow, s.color)?;
        }
        ui.round(rect, radius, self.fill)?;
        match self.border {
            Some(b) => ui.border(rect, radius, b.width, b.color),
            None => Ok(()),
        }
    }
}

/// Which of the theme's fonts a label uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextSize {
    #[default]
    Body,
    /// Body size, bold: button labels and headings.
    Strong,
    /// Values, hints and tooltips.
    Small,
}

/// What a label does with text wider than its rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Overflow {
    /// Cut short with an ellipsis.
    #[default]
    Ellipsis,
    /// Wrapped at words, lines this far apart, from the top of the rect.
    Wrap(f32),
    /// Drawn past the rect.
    Visible,
}

/// A line of text, or a wrapped paragraph.
#[derive(Clone, Copy, Debug)]
pub struct Label<'t> {
    pub text: &'t str,
    pub color: Color,
    pub size: TextSize,
    /// A font of the app's own instead of one of the theme's.
    pub font: Option<&'t Font>,
    pub align: Align,
    pub overflow: Overflow,
}

impl<'t> Label<'t> {
    /// `text` in the body font and `color`.
    pub const fn new(text: &'t str, color: Color) -> Self {
        Self {
            text,
            color,
            size: TextSize::Body,
            font: None,
            align: Align::Start,
            overflow: Overflow::Ellipsis,
        }
    }

    pub const fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }

    pub const fn strong(self) -> Self {
        self.size(TextSize::Strong)
    }

    pub const fn small(self) -> Self {
        self.size(TextSize::Small)
    }

    pub const fn font(mut self, font: &'t Font) -> Self {
        self.font = Some(font);
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub const fn centered(self) -> Self {
        self.align(Align::Center)
    }

    pub const fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// The width the text takes on one line.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> f32 {
        let fonts = ui.fonts();
        let font = self.font.unwrap_or_else(|| fonts.of(self.size));
        ui.text_width(self.text, font)
    }
}

impl Component for Label<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let fonts = ui.fonts();
        let font = self.font.unwrap_or_else(|| fonts.of(self.size));
        match self.overflow {
            Overflow::Ellipsis => ui
                .text_clipped(self.text, rect, font, self.color, self.align)
                .map(drop),
            Overflow::Wrap(line) => ui
                .text_wrapped(self.text, rect, font, self.color, line)
                .map(drop),
            Overflow::Visible => ui.text(self.text, rect, font, self.color, self.align),
        }
    }
}

/// What an icon is drawn from.
#[derive(Clone, Copy, Debug)]
pub enum IconSource<'t> {
    /// One of the built-in line icons.
    Builtin(Icon),
    /// Any vector image, `currentColor` taking the icon's colour.
    Vector(&'t VectorImage),
    /// A bitmap, drawn as it is; the colour is ignored.
    Image(&'t Image),
}

impl From<Icon> for IconSource<'_> {
    fn from(icon: Icon) -> Self {
        Self::Builtin(icon)
    }
}

impl<'t> From<&'t VectorImage> for IconSource<'t> {
    fn from(image: &'t VectorImage) -> Self {
        Self::Vector(image)
    }
}

impl<'t> From<&'t Image> for IconSource<'t> {
    fn from(image: &'t Image) -> Self {
        Self::Image(image)
    }
}

/// An icon, fitted and centred in its rect, keeping its shape.
#[derive(Clone, Copy, Debug)]
pub struct IconMark<'t> {
    pub source: IconSource<'t>,
    pub color: Color,
}

impl<'t> IconMark<'t> {
    pub fn new(source: impl Into<IconSource<'t>>, color: Color) -> Self {
        Self {
            source: source.into(),
            color,
        }
    }
}

impl Component for IconMark<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        match self.source {
            IconSource::Builtin(icon) => icon.draw(ui.ctx, rect, self.color),
            IconSource::Vector(image) => image.draw(ui.ctx, rect, self.color),
            IconSource::Image(image) => {
                let (w, h) = (image.width as f32, image.height as f32);
                if w <= 0.0 || h <= 0.0 {
                    return Ok(());
                }
                let scale = (rect.width / w).min(rect.height / h);
                let (dw, dh) = (w * scale, h * scale);
                let at = Rect::new(
                    rect.x + (rect.width - dw) / 2.0,
                    rect.y + (rect.height - dh) / 2.0,
                    dw,
                    dh,
                );
                ui.ctx.draw_image_rect(image, at)
            }
        }
    }
}

/// The glyphs a [`Mark`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkShape {
    Check,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    ChevronRight,
    Cross,
    Minus,
    Plus,
}

/// A small line glyph, drawn in a square centred in its rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub shape: MarkShape,
    pub color: Color,
    /// Line width as a share of the glyph's size.
    pub weight: f32,
    /// How much of the stroke is drawn, `0..=1`, so a tick can draw itself in.
    pub progress: f32,
}

impl Mark {
    pub const fn new(shape: MarkShape, color: Color) -> Self {
        Self {
            shape,
            color,
            weight: 0.12,
            progress: 1.0,
        }
    }

    pub const fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub const fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    fn strokes(self) -> &'static [&'static [(f32, f32)]] {
        match self.shape {
            MarkShape::Check => &[&[(0.20, 0.52), (0.42, 0.72), (0.80, 0.30)]],
            MarkShape::ChevronDown => &[&[(0.25, 0.38), (0.5, 0.63), (0.75, 0.38)]],
            MarkShape::ChevronUp => &[&[(0.25, 0.62), (0.5, 0.37), (0.75, 0.62)]],
            MarkShape::ChevronLeft => &[&[(0.62, 0.25), (0.37, 0.5), (0.62, 0.75)]],
            MarkShape::ChevronRight => &[&[(0.38, 0.25), (0.63, 0.5), (0.38, 0.75)]],
            MarkShape::Cross => &[&[(0.28, 0.28), (0.72, 0.72)], &[(0.72, 0.28), (0.28, 0.72)]],
            MarkShape::Minus => &[&[(0.25, 0.5), (0.75, 0.5)]],
            MarkShape::Plus => &[&[(0.25, 0.5), (0.75, 0.5)], &[(0.5, 0.25), (0.5, 0.75)]],
        }
    }
}

impl Component for Mark {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let side = rect.width.min(rect.height);
        let square = rect.centered(side, side);
        let at = |(x, y): (f32, f32)| Point::new(square.x + x * side, square.y + y * side);
        let width = (side * self.weight).max(1.0);
        for stroke in self.strokes() {
            let mut points = [Point::default(); 4];
            let n = drawn(stroke, self.progress.clamp(0.0, 1.0), &mut points);
            for p in &mut points[..n] {
                *p = at((p.x, p.y));
            }
            if n >= 2 {
                ui.polyline(&points[..n], width, self.color)?;
            }
        }
        Ok(())
    }
}

fn drawn(stroke: &[(f32, f32)], progress: f32, out: &mut [Point; 4]) -> usize {
    let len = |a: (f32, f32), b: (f32, f32)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let total: f32 = stroke.windows(2).map(|w| len(w[0], w[1])).sum();
    let mut left = total * progress;
    let Some(&first) = stroke.first() else {
        return 0;
    };
    out[0] = Point::new(first.0, first.1);
    let mut n = 1;
    for w in stroke.windows(2) {
        if n >= out.len() || left <= 0.0 {
            break;
        }
        let l = len(w[0], w[1]);
        let t = if l > 0.0 { (left / l).min(1.0) } else { 1.0 };
        out[n] = Point::new(
            w[0].0 + (w[1].0 - w[0].0) * t,
            w[0].1 + (w[1].1 - w[0].1) * t,
        );
        n += 1;
        left -= l;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_draws_in_by_length() {
        let mut out = [Point::default(); 4];
        let stroke = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        assert_eq!(drawn(&stroke, 1.0, &mut out), 3);
        assert_eq!(drawn(&stroke, 0.25, &mut out), 2);
        assert_eq!(out[1], Point::new(0.5, 0.0));
        assert_eq!(drawn(&stroke, 0.75, &mut out), 3);
        assert_eq!(out[2], Point::new(1.0, 0.5));
    }

    #[test]
    fn fading_a_surface_fades_every_part() {
        let s = Surface::new(Color::rgb(10, 10, 10))
            .border(1.0, Color::rgb(20, 20, 20))
            .shadow(Shadow::new(Color::rgba(0, 0, 0, 100)))
            .fade(0.5);
        assert_eq!(s.fill.a, 128);
        assert_eq!(s.border.map(|b| b.color.a), Some(128));
        assert_eq!(s.shadow.map(|s| s.color.a), Some(50));
    }
}
