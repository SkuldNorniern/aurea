//! Display pieces for applications that own their input and selection state.

use super::{Component, Label, Surface};
use crate::AureaResult;
use crate::render::{Color, Font, Rect};
use crate::ui::{Align, Ui, WidgetId};

/// A prompt followed by text and an optional caret at its end.
#[derive(Clone, Copy, Debug)]
pub struct PromptLine<'t> {
    pub prompt: &'t str,
    pub text: &'t str,
    pub font: Option<&'t Font>,
    pub color: Color,
    pub prompt_color: Color,
    pub caret: bool,
}

impl<'t> PromptLine<'t> {
    pub const fn new(prompt: &'t str, text: &'t str, color: Color, prompt_color: Color) -> Self {
        Self {
            prompt,
            text,
            font: None,
            color,
            prompt_color,
            caret: true,
        }
    }

    pub const fn font(mut self, font: &'t Font) -> Self {
        self.font = Some(font);
        self
    }

    pub const fn caret(mut self, caret: bool) -> Self {
        self.caret = caret;
        self
    }

    /// Paints the line and returns the x just after the text/caret, clamped to the rect, so the
    /// caller can place trailing content.
    pub fn paint_end<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<f32> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(rect.x);
        }
        let fonts = ui.fonts();
        let font = self.font.unwrap_or(&fonts.text);
        let caret_width = if self.caret {
            3.0_f32.min(rect.width)
        } else {
            0.0
        };
        let prompt_width = if self.prompt.is_empty() {
            0.0
        } else {
            ui.measure(self.prompt, font).advance + ui.measure(" ", font).advance
        }
        .min((rect.width - caret_width).max(0.0));
        let available = (rect.width - prompt_width - caret_width).max(0.0);
        let text_width = ui.measure(self.text, font).advance;
        let end = rect.x + prompt_width + text_width.min(available);
        ui.clipped(rect, |ui| {
            if prompt_width > 0.0 {
                Label::new(self.prompt, self.prompt_color)
                    .font(font)
                    .paint(ui, Rect::new(rect.x, rect.y, prompt_width, rect.height))?;
            }
            let text_rect = Rect::new(rect.x + prompt_width, rect.y, available, rect.height);
            if available > 0.0 {
                ui.clipped(text_rect, |ui| {
                    let at = Rect::new(
                        text_rect.x - (text_width - available).max(0.0),
                        rect.y,
                        text_width,
                        rect.height,
                    );
                    ui.text(self.text, at, font, self.color, Align::Start)
                })?;
            }
            if self.caret {
                ui.fill(
                    Rect::new(
                        end + caret_width / 3.0,
                        rect.y + rect.height * 0.2,
                        caret_width * 2.0 / 3.0,
                        rect.height * 0.6,
                    ),
                    self.color,
                )?;
            }
            Ok(())
        })?;
        Ok(end + caret_width)
    }
}

impl Component for PromptLine<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        self.paint_end(ui, rect).map(drop)
    }
}

/// A primary label and an optional right-aligned detail.
///
/// ```rust,no_run
/// use aurea::ui::{Component, DetailRow, ListView, Ui};
/// # fn draw(ui: &mut Ui<'_, u8>, offset: &mut f32) -> aurea::AureaResult<()> {
/// let names = [("main.rs", "src"), ("Cargo.toml", "workspace")];
/// let colors = ui.theme.palette;
/// ListView::new(offset, names.len(), 28.0).show(
///     ui, 1, aurea::Rect::new(0.0, 0.0, 300.0, 200.0), |ui, row| {
///         let (name, detail) = names[row.index];
///         DetailRow::new(name, detail, colors.text, colors.dim).paint(ui, row.rect)
///     },
/// )?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct DetailRow<'t> {
    pub primary: &'t str,
    pub detail: &'t str,
    pub font: Option<&'t Font>,
    pub color: Color,
    pub detail_color: Color,
    pub selection: Option<Color>,
    pub padding: f32,
    pub gap: f32,
    pub radius: f32,
}

impl<'t> DetailRow<'t> {
    pub const fn new(primary: &'t str, detail: &'t str, color: Color, detail_color: Color) -> Self {
        Self {
            primary,
            detail,
            font: None,
            color,
            detail_color,
            selection: None,
            padding: 8.0,
            gap: 16.0,
            radius: 6.0,
        }
    }

    pub const fn font(mut self, font: &'t Font) -> Self {
        self.font = Some(font);
        self
    }

    pub const fn selected(mut self, color: Color) -> Self {
        self.selection = Some(color);
        self
    }
}

impl Component for DetailRow<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        ui.clipped(rect, |ui| {
            if let Some(color) = self.selection {
                Surface::new(color).radius(self.radius).paint(ui, rect)?;
            }
            let fonts = ui.fonts();
            let font = self.font.unwrap_or(&fonts.text);
            let content = rect.inset_xy(self.padding.max(0.0).min(rect.width / 2.0), 0.0);
            let primary = Label::new(self.primary, self.color).font(font);
            let detail = Label::new(self.detail, self.detail_color)
                .font(font)
                .align(Align::End);
            let detail_width = detail.width(ui);
            let show_detail = !self.detail.is_empty()
                && primary.width(ui) + self.gap.max(0.0) + detail_width <= content.width;
            let primary_width = if show_detail {
                content.width - detail_width - self.gap.max(0.0)
            } else {
                content.width
            };
            if primary_width > 0.0 {
                primary.paint(
                    ui,
                    Rect::new(content.x, content.y, primary_width, content.height),
                )?;
            }
            if show_detail {
                detail.paint(ui, content)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Theme, UiState};

    const INK: Color = Color::rgb(255, 255, 255);
    const DETAIL: Color = Color::rgb(255, 0, 0);

    #[test]
    fn long_unicode_input_keeps_the_caret_and_ink_inside_the_rect() {
        let mut raster = CpuRasterizer::new(160, 50);
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        let rect = Rect::new(20.0, 10.0, 100.0, 30.0);
        {
            let mut ctx = raster.begin_frame().expect("frame");
            let mut ui = state.frame(&mut *ctx, &theme);
            let end = PromptLine::new("find:", &"한글🙂abcdef".repeat(30), INK, INK)
                .paint_end(&mut ui, rect)
                .expect("prompt");
            assert!((end - rect.right()).abs() < 0.01);
        }
        raster.end_frame().expect("end");
        assert_ne!(raster.pixels()[25 * 160 + 118], 0, "caret remains visible");
        for (i, &pixel) in raster.pixels().iter().enumerate() {
            if !(20..120).contains(&(i % 160)) || !(10..40).contains(&(i / 160)) {
                assert_eq!(pixel, 0, "ink escaped the prompt at {i}");
            }
        }
    }

    #[test]
    fn details_are_right_aligned_and_omitted_when_the_primary_needs_the_room() {
        let render = |primary: &str| {
            let mut raster = CpuRasterizer::new(240, 40);
            let mut state = UiState::<u8>::new();
            let theme = Theme::dark();
            {
                let mut ctx = raster.begin_frame().expect("frame");
                let mut ui = state.frame(&mut *ctx, &theme);
                DetailRow::new(primary, "detail", INK, DETAIL)
                    .paint(&mut ui, Rect::new(10.0, 4.0, 200.0, 30.0))
                    .expect("row");
            }
            raster.end_frame().expect("end");
            raster.pixels().to_vec()
        };
        let is_red = |pixel: &u32| pixel & 0x00ff0000 != 0 && pixel & 0x0000ffff == 0;
        let short = render("name");
        assert!(short.iter().any(is_red), "detail fits");
        assert!(
            short
                .iter()
                .enumerate()
                .filter(|(_, p)| is_red(p))
                .all(|(i, _)| i % 240 > 140),
            "detail hugs the right edge"
        );
        let long = render(&"long 한글 name ".repeat(40));
        assert!(!long.iter().any(is_red), "overlapping detail is hidden");
        for (i, &pixel) in long.iter().enumerate() {
            if !(10..210).contains(&(i % 240)) || !(4..34).contains(&(i / 240)) {
                assert_eq!(pixel, 0, "ink escaped the row at {i}");
            }
        }
    }

    #[test]
    fn empty_and_tiny_rects_do_not_leak_or_panic() {
        let mut raster = CpuRasterizer::new(80, 40);
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        {
            let mut ctx = raster.begin_frame().expect("frame");
            let mut ui = state.frame(&mut *ctx, &theme);
            for width in [-5.0, 0.0, 1.0, 2.0] {
                let rect = Rect::new(20.0, 10.0, width, 20.0);
                let end = PromptLine::new("very long prompt", "value", INK, INK)
                    .paint_end(&mut ui, rect)
                    .expect("tiny prompt");
                assert!(end >= rect.x && end <= rect.x + width.max(0.0));
                DetailRow::new("primary", "detail", INK, DETAIL)
                    .selected(INK)
                    .paint(&mut ui, rect)
                    .expect("tiny row");
            }
        }
        raster.end_frame().expect("end");
        assert!(raster.pixels().iter().enumerate().all(
            |(i, &p)| p == 0 || ((20..22).contains(&(i % 80)) && (10..30).contains(&(i / 80)))
        ));
    }
}
