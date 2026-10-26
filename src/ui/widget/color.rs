//! A colour picker: a saturation and value square, a hue strip, an optional alpha strip, and an
//! optional hex field.

use std::fmt::Write as _;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Hsv, Point, Rect};
use crate::ui::component::{EditableText, FocusRing};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{AlphaStrip, Checker, Component, HueStrip, PickRing, Surface, SvField};
use crate::ui::state::{Interaction, Sense, WidgetId};
use crate::ui::text_edit::Rules;
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How a colour picker looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickerStyle {
    /// Between the square, the strips and the hex row.
    pub gap: f32,
    /// How wide a strip is.
    pub strip: f32,
    /// Corner radius of the square and strips.
    pub radius: f32,
    /// Outline colour.
    pub border: Color,
    /// Checkerboard behind transparent colour.
    pub checker: Checker,
    /// The picked spot's ring.
    pub ring: f32,
    /// The hex field's face and text.
    pub field: Color,
    /// Hex text colour.
    pub text: Color,
}

impl PickerStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            gap: 8.0,
            strip: 16.0,
            radius: 4.0,
            border: p.border,
            checker: Checker::new(),
            ring: 6.0,
            field: p.raised,
            text: p.text,
        }
    }
}

/// Picks a colour.
///
/// ```rust,no_run
/// # use aurea::render::Hsv;
/// # use aurea::ui::{ColorPicker, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, brush: &mut Hsv, hex: &mut String) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 240.0, 220.0);
/// if ColorPicker::new(brush).alpha(true).hex_field(hex).show(ui, 4, at)?.changed {
///     let color = brush.to_color();
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ColorPicker<'v> {
    value: &'v mut Hsv,
    alpha: bool,
    hex: Option<&'v mut String>,
    style: Option<PickerStyle>,
    enabled: bool,
}

const SQUARE: u16 = 0;
const HUE: u16 = 1;
const ALPHA: u16 = 2;
const HEX: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    square: Rect,
    hue: Rect,
    alpha: Option<Rect>,
    hex: Option<Rect>,
}

impl<'v> ColorPicker<'v> {
    /// Creates a picker editing `value`.
    pub fn new(value: &'v mut Hsv) -> Self {
        Self {
            value,
            alpha: false,
            hex: None,
            style: None,
            enabled: true,
        }
    }

    /// Shows the alpha strip, so the colour can be made see-through.
    pub fn alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// A hex field along the bottom, editing `text`.
    pub fn hex_field(mut self, text: &'v mut String) -> Self {
        self.hex = Some(text);
        self
    }

    /// Sets the look; defaults to the theme's.
    pub fn style(mut self, style: PickerStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether the picker takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the picker in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    fn layout(&self, rect: Rect, style: &PickerStyle, row: f32) -> Layout {
        let (hex, body) = if self.hex.is_some() {
            let field = Rect::new(rect.x, rect.bottom() - row, rect.width, row);
            let rest = Rect::new(
                rect.x,
                rect.y,
                rect.width,
                (rect.height - row - style.gap).max(0.0),
            );
            (Some(field), rest)
        } else {
            (None, rect)
        };
        let strips = if self.alpha { 2.0 } else { 1.0 };
        let side = strips * (style.strip + style.gap);
        let square = Rect::new(body.x, body.y, (body.width - side).max(0.0), body.height);
        let strip_at = |n: f32| {
            let x = square.right() + style.gap + n * (style.strip + style.gap);
            Rect::new(x, body.y, style.strip, body.height)
        };
        Layout {
            square,
            hue: strip_at(0.0),
            alpha: self.alpha.then(|| strip_at(1.0)),
            hex,
        }
    }
}

impl<Id: WidgetId> Widget<Id> for ColorPicker<'_> {
    fn show(mut self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| PickerStyle::new(ui.theme));
        let at = self.layout(rect, &style, ui.theme.control_height);
        let before = *self.value;
        let mut response = Response::new(rect, Interaction::default());

        let square = ui.interact(id, SQUARE, at.square, Sense::Adjust, self.enabled);
        let hue = ui.interact(id, HUE, at.hue, Sense::Adjust, self.enabled);
        follow_square(ui, square, at.square, self.value);
        follow_strip(ui, hue, at.hue, &mut self.value.h, 360.0);
        merge(&mut response, square);
        merge(&mut response, hue);
        let alpha = at.alpha.map(|strip| {
            let input = ui.interact(id, ALPHA, strip, Sense::Adjust, self.enabled);
            let mut clear = 1.0 - self.value.a;
            follow_strip(ui, input, strip, &mut clear, 1.0);
            self.value.a = 1.0 - clear;
            merge(&mut response, input);
            (strip, input)
        });
        if let (Some(field), Some(text)) = (at.hex, self.hex.as_deref_mut()) {
            let input = ui.interact(id, HEX, field, Sense::Text, self.enabled);
            hex(ui, (id, HEX), input, field, text, self.value, &style)?;
            merge(&mut response, input);
        }

        if *self.value != before {
            response.changed = true;
            ui.mark_changed();
        }
        paint(ui, id, &at, (square, hue, alpha), *self.value, &style)?;
        Ok(response)
    }
}

fn merge(response: &mut Response, input: Interaction) {
    response.hovered |= input.hovered;
    response.held |= input.held;
    response.focused |= input.focused;
}

fn step<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    if ui.modifiers().shift { 0.1 } else { 0.01 }
}

fn follow_square<Id: WidgetId>(ui: &Ui<'_, Id>, input: Interaction, rect: Rect, value: &mut Hsv) {
    if input.held
        && let Some(p) = ui.pointer()
    {
        value.s = fraction(p.x - rect.x, rect.width);
        value.v = 1.0 - fraction(p.y - rect.y, rect.height);
    }
    if !input.focused {
        return;
    }
    let by = step(ui);
    for (key, _) in ui.keys() {
        match key {
            KeyCode::Left => value.s = (value.s - by).max(0.0),
            KeyCode::Right => value.s = (value.s + by).min(1.0),
            KeyCode::Up => value.v = (value.v + by).min(1.0),
            KeyCode::Down => value.v = (value.v - by).max(0.0),
            _ => {}
        }
    }
}

fn follow_strip<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    input: Interaction,
    rect: Rect,
    value: &mut f32,
    span: f32,
) {
    if input.held
        && let Some(p) = ui.pointer()
    {
        *value = fraction(p.y - rect.y, rect.height) * span;
    }
    if !input.focused {
        return;
    }
    let by = step(ui) * span;
    for (key, _) in ui.keys() {
        match key {
            KeyCode::Up | KeyCode::Left => *value = (*value - by).max(0.0),
            KeyCode::Down | KeyCode::Right => *value = (*value + by).min(span),
            KeyCode::Home => *value = 0.0,
            KeyCode::End => *value = span,
            _ => {}
        }
    }
}

fn fraction(offset: f32, length: f32) -> f32 {
    if length <= 0.0 {
        0.0
    } else {
        (offset / length).clamp(0.0, 1.0)
    }
}

fn hex<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    key: (Id, u16),
    input: Interaction,
    field: Rect,
    text: &mut String,
    value: &mut Hsv,
    style: &PickerStyle,
) -> AureaResult<()> {
    if !input.focused {
        text.clear();
        let _ = write!(text, "{}", value.to_color().hex());
    }
    Surface::new(style.field)
        .radius(style.radius)
        .border(1.0, style.border)
        .paint(ui, field)?;
    let accept = |c: char| c == '#' || c.is_ascii_hexdigit();
    let rules = Rules {
        max_chars: Some(9),
        accept: Some(&accept),
        ..Rules::default()
    };
    let inner = field.inset_xy(8.0, 0.0);
    let outcome = EditableText::new(text)
        .rules(rules)
        .show(ui, key, inner, input)?;
    if outcome.changed
        && let Some(color) = Color::from_hex(text)
    {
        *value = keep_hue(color.to_hsv(), value.h);
    }
    Ok(())
}

fn keep_hue(mut typed: Hsv, hue: f32) -> Hsv {
    if typed.s <= 0.0 || typed.v <= 0.0 {
        typed.h = hue;
    }
    typed
}

fn paint<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    at: &Layout,
    (square, hue, alpha): (Interaction, Interaction, Option<(Rect, Interaction)>),
    value: Hsv,
    style: &PickerStyle,
) -> AureaResult<()> {
    let enabled = ui.eased(id, SQUARE, square).enabled;
    let border = style.border.fade(enabled);
    SvField { hue: value.h }.paint(ui, at.square)?;
    ui.border(at.square, 0.0, 1.0, border)?;
    let spot = Point::new(
        at.square.x + value.s * at.square.width,
        at.square.y + (1.0 - value.v) * at.square.height,
    );
    PickRing {
        at: spot,
        radius: style.ring,
    }
    .paint(ui, at.square)?;
    strip(
        ui,
        id,
        (HUE, hue),
        at.hue,
        value.h / 360.0,
        style,
        |ui, rect| HueStrip { vertical: true }.paint(ui, rect),
    )?;
    if let Some((rect, input)) = alpha {
        let color = value.to_color();
        let checker = style.checker;
        strip(
            ui,
            id,
            (ALPHA, input),
            rect,
            1.0 - value.a,
            style,
            |ui, rect| {
                AlphaStrip {
                    color,
                    checker,
                    vertical: true,
                }
                .paint(ui, rect)
            },
        )?;
    }
    let ring = ui.eased(id, SQUARE, square).focus;
    FocusRing::new(ui.theme, 0.0, ring).paint(ui, at.square)
}

fn strip<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    (part, input): (u16, Interaction),
    rect: Rect,
    t: f32,
    style: &PickerStyle,
    fill: impl FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<()>,
) -> AureaResult<()> {
    let state = ui.eased(id, part, input);
    FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
    fill(ui, rect)?;
    ui.border(rect, 0.0, 1.0, style.border.fade(state.enabled))?;
    let y = rect.y + t.clamp(0.0, 1.0) * rect.height;
    let mark = Rect::new(rect.x - 2.0, y - 2.0, rect.width + 4.0, 4.0);
    ui.border(mark.inset(-1.0), 3.0, 1.0, Color::rgba(0, 0, 0, 140))?;
    ui.border(mark, 2.0, 2.0, Color::rgb(255, 255, 255))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    const RECT: Rect = Rect::new(0.0, 0.0, 240.0, 220.0);

    struct Picker {
        rig: Rig,
        value: Hsv,
        hex: String,
    }

    impl Picker {
        fn new() -> Self {
            let mut picker = Self {
                rig: Rig::new(240, 220).still(),
                value: Hsv::new(120.0, 0.5, 0.5, 1.0),
                hex: String::new(),
            };
            picker.frame();
            picker
        }

        fn frame(&mut self) -> Response {
            self.rig.frame(|ui| {
                ColorPicker::new(&mut self.value)
                    .alpha(true)
                    .hex_field(&mut self.hex)
                    .show(ui, 0, RECT)
                    .expect("draw")
            })
        }

        fn layout(&self) -> Layout {
            let style = PickerStyle::new(&self.rig.theme);
            let mut hex = String::new();
            let mut value = self.value;
            ColorPicker::new(&mut value)
                .alpha(true)
                .hex_field(&mut hex)
                .layout(RECT, &style, self.rig.theme.control_height)
        }

        fn press(&mut self, at: Point, pressed: bool) {
            self.rig.at(at.x, at.y);
            self.rig.button(at.x, at.y, pressed);
        }
    }

    #[test]
    fn the_parts_share_the_rect() {
        let at = Picker::new().layout();
        let hex = at.hex.expect("hex row");
        let alpha = at.alpha.expect("alpha strip");
        assert!(at.square.right() < at.hue.x && at.hue.right() < alpha.x);
        assert!(alpha.right() <= RECT.right() + 0.5);
        assert!(at.square.bottom() < hex.y && hex.bottom() <= RECT.bottom());
    }

    #[test]
    fn pressing_the_square_sets_saturation_and_value() {
        let mut rig = Picker::new();
        let square = rig.layout().square;
        rig.press(Point::new(square.right(), square.y), true);
        assert!(rig.frame().changed);
        assert_eq!((rig.value.s, rig.value.v), (1.0, 1.0));
        assert_eq!(rig.value.h, 120.0, "the hue stays");
    }

    #[test]
    fn the_hue_strip_runs_down_round_the_circle() {
        let mut rig = Picker::new();
        let hue = rig.layout().hue;
        rig.press(Point::new(hue.x + 4.0, hue.y + hue.height / 2.0), true);
        rig.frame();
        assert!((rig.value.h - 180.0).abs() < 1.0, "got {}", rig.value.h);
    }

    #[test]
    fn the_hex_field_follows_the_colour() {
        let mut rig = Picker::new();
        rig.frame();
        assert_eq!(rig.hex, rig.value.to_color().hex().to_string());
    }

    #[test]
    fn typed_grey_keeps_the_hue() {
        let grey = Color::rgb(90, 90, 90).to_hsv();
        assert_eq!(keep_hue(grey, 200.0).h, 200.0);
        let red = Color::rgb(255, 0, 0).to_hsv();
        assert_eq!(keep_hue(red, 200.0).h, 0.0);
    }

    #[test]
    fn fractions_stay_inside() {
        assert_eq!(fraction(-5.0, 100.0), 0.0);
        assert_eq!(fraction(150.0, 100.0), 1.0);
        assert_eq!(fraction(5.0, 0.0), 0.0);
    }
}
