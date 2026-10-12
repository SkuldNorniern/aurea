//! A segmented control: one [`Surface`], a sliding highlight, and a part
//! per option that takes clicks.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a segmented control looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentedStyle {
    pub background: Color,
    pub highlight: Color,
    pub text: StateColors,
    pub picked_text: Color,
    pub radius: f32,
    /// Space between the highlight and the control's edge.
    pub inset: f32,
    pub text_size: TextSize,
}

impl SegmentedStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            background: p.raised,
            highlight: p.accent,
            text: StateColors::new(p.dim, p.text, p.text),
            picked_text: p.on_accent,
            radius: theme.radius,
            inset: 2.0,
            text_size: TextSize::Strong,
        }
    }
}

/// A row of options, one picked, the highlight sliding between them.
#[derive(Debug)]
pub struct Segmented<'v> {
    options: &'v [&'v str],
    selected: &'v mut usize,
    style: Option<SegmentedStyle>,
    enabled: bool,
}

impl<'v> Segmented<'v> {
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            options,
            selected,
            style: None,
            enabled: true,
        }
    }

    pub fn style(mut self, style: SegmentedStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<Id: WidgetId> Widget<Id> for Segmented<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let n = self.options.len();
        if n == 0 {
            return Ok(response);
        }
        let style = self.style.unwrap_or_else(|| SegmentedStyle::new(ui.theme));
        let fade = if self.enabled {
            1.0
        } else {
            ui.theme.disabled_alpha
        };
        Surface::new(style.background.fade(fade))
            .radius(style.radius)
            .paint(ui, rect)?;
        // The highlight's position is kept on a part no cell uses.
        #[allow(clippy::cast_precision_loss)]
        let target = (*self.selected).min(n - 1) as f32;
        let at = ui.animate(id, u16::MAX, Channel::Value, target, ui.theme.motion.slide);
        let width = rect.column(0, n, 0.0).width;
        let moving = Rect::new(rect.x + width * at, rect.y, width, rect.height);
        let inner = (style.radius - style.inset).max(0.0);
        Surface::new(style.highlight.fade(fade))
            .radius(inner)
            .paint(ui, moving.inset(style.inset))?;
        for (i, option) in self.options.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(u16::MAX - 1);
            let cell = rect.column(i, n, 0.0);
            let input = ui.interact(id, part, cell, Sense::Click, self.enabled);
            let state = ui.eased(id, part, input);
            FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, cell)?;
            if input.clicked && *self.selected != i {
                *self.selected = i;
                response.changed = true;
                ui.mark_changed();
            }
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            #[allow(clippy::cast_precision_loss)]
            let picked = (1.0 - (at - i as f32).abs()).clamp(0.0, 1.0);
            let color = style
                .text
                .at(state)
                .mix(style.picked_text.fade(fade), picked);
            Label::new(option, color)
                .size(style.text_size)
                .centered()
                .paint(ui, cell.inset_xy(6.0, 0.0))?;
        }
        Ok(response)
    }
}
