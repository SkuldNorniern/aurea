//! The tooltip asked for this frame: a [`card`] with a [`Label`], drawn over everything as the
//! frame ends.

use std::mem;
use std::time::Instant;

use crate::AureaResult;
use crate::render::Rect;
use crate::ui::component::card;
use crate::ui::frame::Ui;
use crate::ui::paint::{Component, Label, Overflow};
use crate::ui::state::WidgetId;

const DELAY: f32 = 0.45;
const FADE: f32 = 0.12;

const WIDEST: f32 = 320.0;

pub(crate) fn draw<Id: WidgetId>(ui: &mut Ui<'_, Id>) -> AureaResult<()> {
    let Some(anchor) = ui.state.tip.anchor else {
        ui.state.tip.since = None;
        return Ok(());
    };
    let since = match ui.state.tip.since {
        Some((rect, since)) if rect == anchor => since,
        _ => {
            let now = Instant::now();
            ui.state.tip.since = Some((anchor, now));
            now
        }
    };
    let shown = (since.elapsed().as_secs_f32() - DELAY) / FADE;
    if shown < 1.0 {
        ui.keep_animating();
    }
    if shown <= 0.0 {
        return Ok(());
    }
    let text = mem::take(&mut ui.state.tip.text);
    let result = place(ui, anchor, &text, shown.min(1.0));
    ui.state.tip.text = text;
    result
}

fn place<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    anchor: Rect,
    text: &str,
    alpha: f32,
) -> AureaResult<()> {
    let fonts = ui.fonts();
    let font = &fonts.small;
    let pad = 8.0;
    let line = font.size * 1.45;
    #[allow(clippy::cast_precision_loss)]
    let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
    let widest = WIDEST.min(canvas.width - pad * 4.0).max(40.0);
    let full = ui.text_width(text, font);
    let lines = if full <= widest {
        1.0
    } else {
        (full / (widest * 0.9)).ceil()
    };
    let (w, h) = (full.min(widest) + pad * 2.0, line * lines + pad * 1.2);
    let gap = 6.0;
    let x = (anchor.center().x - w / 2.0).clamp(gap, (canvas.right() - w - gap).max(gap));
    let y = if anchor.bottom() + gap + h <= canvas.bottom() {
        anchor.bottom() + gap
    } else {
        (anchor.y - gap - h).max(gap)
    };
    let rect = Rect::new(x, y, w, h);
    card(ui.theme).fade(alpha).paint(ui, rect)?;
    let body = Rect::new(
        rect.x + pad,
        rect.y + pad * 0.6,
        w - pad * 2.0,
        line * lines,
    );
    let color = ui.theme.palette.text.fade(alpha);
    let overflow = if lines > 1.0 {
        Overflow::Wrap(line)
    } else {
        Overflow::Visible
    };
    Label::new(text, color)
        .small()
        .overflow(overflow)
        .paint(ui, body)
}
