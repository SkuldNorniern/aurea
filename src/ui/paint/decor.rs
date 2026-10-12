//! Small pieces that say something without taking input: a line between
//! groups, a badge with a count or a state, and a spinner for work going on.

use std::f32::consts::{FRAC_PI_2, TAU};

use super::{Component, Label, Surface, TextSize};
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::frame::Ui;
use crate::ui::state::WidgetId;
use crate::ui::theme::Theme;

/// A thin line across the middle of its rect, along its longer side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Separator {
    pub color: Color,
    pub width: f32,
    /// Space kept clear at both ends.
    pub inset: f32,
}

impl Separator {
    pub fn new(theme: &Theme) -> Self {
        Self {
            color: theme.palette.border,
            width: theme.border_width,
            inset: 0.0,
        }
    }

    #[must_use]
    pub const fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }
}

impl Component for Separator {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let c = rect.center();
        let line = if rect.width >= rect.height {
            Rect::new(
                rect.x + self.inset,
                c.y - self.width / 2.0,
                (rect.width - self.inset * 2.0).max(0.0),
                self.width,
            )
        } else {
            Rect::new(
                c.x - self.width / 2.0,
                rect.y + self.inset,
                self.width,
                (rect.height - self.inset * 2.0).max(0.0),
            )
        };
        ui.fill(line, self.color)
    }
}

/// What a badge says about something.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeTone {
    #[default]
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

/// A count or a short state in a pill: "3", "Beta", "Offline". Sized to its
/// text by [`Badge::width`], centred in the rect it is painted in.
#[derive(Clone, Copy, Debug)]
pub struct Badge<'t> {
    pub text: &'t str,
    pub tone: BadgeTone,
    /// Filled with the tone's colour; otherwise a tint of it.
    pub solid: bool,
}

impl<'t> Badge<'t> {
    pub const fn new(text: &'t str) -> Self {
        Self {
            text,
            tone: BadgeTone::Neutral,
            solid: false,
        }
    }

    #[must_use]
    pub const fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub const fn solid(mut self) -> Self {
        self.solid = true;
        self
    }

    /// How wide it is drawn: its text and a little room either side.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> f32 {
        let fonts = ui.fonts();
        let text = ui.text_width(self.text, &fonts.small);
        (text + fonts.small.size * 1.3).max(fonts.small.size * 1.7)
    }

    fn colors(&self, theme: &Theme) -> (Color, Color) {
        let p = theme.palette;
        let ink = match self.tone {
            BadgeTone::Neutral => p.dim,
            BadgeTone::Accent => p.accent,
            BadgeTone::Success => p.success,
            BadgeTone::Warning => p.warning,
            BadgeTone::Danger => p.danger,
        };
        if self.solid {
            let text = if self.tone == BadgeTone::Accent {
                p.on_accent
            } else {
                p.background
            };
            (ink, text)
        } else {
            (ink.fade(0.18), ink)
        }
    }
}

impl Component for Badge<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let height = (ui.theme.small_size * 1.7).min(rect.height);
        let width = self.width(ui).min(rect.width);
        let at = rect.centered(width, height);
        let (fill, ink) = self.colors(ui.theme);
        Surface::new(fill).pill().paint(ui, at)?;
        Label::new(self.text, ink)
            .size(TextSize::Small)
            .centered()
            .paint(ui, at)
    }
}

/// Work going on: dots round a circle, the brightest one going round.
/// Asks for frames while it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spinner {
    pub color: Color,
    /// Turns a second.
    pub speed: f32,
}

impl Spinner {
    pub fn new(theme: &Theme) -> Self {
        Self {
            color: theme.palette.accent,
            speed: 1.1,
        }
    }
}

/// Dots in a spinner.
const DOTS: u8 = 10;

impl Component for Spinner {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        ui.keep_animating();
        let side = rect.width.min(rect.height);
        let c = rect.center();
        let ring = side * 0.38;
        let dot = side * 0.075;
        let head = (ui.time() * self.speed).fract();
        for i in 0..DOTS {
            let t = f32::from(i) / f32::from(DOTS);
            let angle = t * TAU - FRAC_PI_2;
            let at = Point::new(c.x + ring * angle.cos(), c.y + ring * angle.sin());
            // Brightest at the head, fading behind it.
            let behind = (head - t).rem_euclid(1.0);
            let alpha = 0.15 + 0.85 * (1.0 - behind).powi(2);
            ui.circle(
                at,
                dot * (0.7 + 0.3 * (1.0 - behind)),
                self.color.fade(alpha),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;

    #[test]
    fn a_badge_is_tinted_unless_solid() {
        let theme = Theme::dark();
        let p = theme.palette;
        let (fill, ink) = Badge::new("3").tone(BadgeTone::Danger).colors(&theme);
        assert_eq!(ink, p.danger);
        assert_eq!(fill, p.danger.fade(0.18));
        let (fill, ink) = Badge::new("New")
            .tone(BadgeTone::Accent)
            .solid()
            .colors(&theme);
        assert_eq!((fill, ink), (p.accent, p.on_accent));
    }

    #[test]
    fn a_spinner_keeps_frames_coming_and_a_badge_fits_its_text() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(120, 60);
        let mut state = UiState::<u8>::new();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            Spinner::new(&theme)
                .paint(&mut ui, Rect::new(0.0, 0.0, 24.0, 24.0))
                .expect("spinner");
            let short = Badge::new("3").width(&mut ui);
            let long = Badge::new("Offline").width(&mut ui);
            assert!(long > short && short >= theme.small_size * 1.7 - 0.01);
            Separator::new(&theme)
                .paint(&mut ui, Rect::new(0.0, 40.0, 120.0, 9.0))
                .expect("separator");
        }
        drop(ctx);
        raster.end_frame().expect("end");
        assert!(state.wants_frame());
    }
}
