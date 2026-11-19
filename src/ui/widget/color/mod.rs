//! A colour picker: a saturation and value square, a hue strip, an optional alpha strip, and an
//! optional hex field.

pub mod swatch;
pub mod wheel;

use std::fmt::Write as _;

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Hsv, Point, Rect};
use crate::ui::accessibility::Role;
use crate::ui::component::{EditableText, FocusRing};
use crate::ui::frame::{Eased, Response, Ui};
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

/// What a [`ColorPicker`] draws, from [`ColorPicker::look`].
#[derive(Debug)]
pub struct PickerParts<'v> {
    /// The colour, after this frame's input and any typed hex once [`Look::paint_hex`] has run.
    pub value: Hsv,
    /// The saturation and value square.
    pub square: Rect,
    /// The hue strip.
    pub hue: Rect,
    /// The alpha strip, when it is shown.
    pub alpha: Option<Rect>,
    /// The hex field, when it is shown; [`Look::paint_hex`] draws it.
    pub hex: Option<Rect>,
    /// The handle in the square, at the saturation and value picked.
    pub spot: Point,
    /// The mark on the hue strip, at the hue picked.
    pub hue_mark: Rect,
    /// The mark on the alpha strip, at the alpha picked, when it is shown.
    pub alpha_mark: Option<Rect>,
    /// Focus and enabled of the hue strip.
    pub hue_state: Eased,
    /// Focus and enabled of the alpha strip, when it is shown.
    pub alpha_state: Option<Eased>,
    /// The style it draws with, the theme's unless one was given.
    pub style: PickerStyle,
    /// The hex text the field edits, while it is shown.
    hex_text: Option<&'v mut String>,
    /// The hex field's input this frame.
    hex_input: Interaction,
}

const SQUARE: u16 = 0;
const HUE: u16 = 1;
const ALPHA: u16 = 2;
const HEX: u16 = 3;
/// The square's second axis, for screen readers: the square itself is brightness.
const SATURATION: u16 = 4;

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

impl<'v> ColorPicker<'v> {
    /// Takes the input as [`ColorPicker::show`] does and works out what it would draw, without
    /// drawing: the app draws it, or [`Look::paint_default`] does for the square and strips.
    /// [`Look::paint_hex`] draws the hex field and takes its typing; call it first so the square
    /// shows the colour typed this frame.
    pub fn look<Id: WidgetId>(
        mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<PickerParts<'v>> {
        let style = self.style.unwrap_or_else(|| PickerStyle::new(ui.theme));
        let at = self.layout(rect, &style, ui.theme.control_height);
        let before = *self.value;
        let mut response = Response::new(rect, Interaction::default());

        // under the square, so the square keeps the pointer
        ui.accessible_region(id, SATURATION, at.square);
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
        let hex_text = self.hex.take();
        let hex_input = at.hex.filter(|_| hex_text.is_some()).map(|field| {
            let input = ui.interact(id, HEX, field, Sense::Text, self.enabled);
            merge(&mut response, input);
            input
        });
        let hex = hex_input.and(hex_text.as_deref().map(String::as_str));
        self.accessibility(ui, id, hex);

        if *self.value != before {
            response.changed = true;
            ui.mark_changed();
        }
        let value = *self.value;
        let (spot, hue_mark, alpha_mark) = marks(value, at.square, at.hue, at.alpha);
        Look {
            rect,
            state: ui.eased(id, SQUARE, square),
            held_at: ui.held_at(id, SQUARE),
            response,
            parts: PickerParts {
                value,
                square: at.square,
                hue: at.hue,
                alpha: at.alpha,
                hex: at.hex,
                spot,
                hue_mark,
                alpha_mark,
                hue_state: ui.eased(id, HUE, hue),
                alpha_state: alpha.map(|(_, input)| ui.eased(id, ALPHA, input)),
                style,
                hex_text,
                hex_input: hex_input.unwrap_or_default(),
            },
        }
    }
}

impl ColorPicker<'_> {
    /// Saturation, brightness, hue and opacity as sliders, each stepping by a twentieth.
    fn accessibility<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id, hex: Option<&str>) {
        let v = &mut *self.value;
        ui.accessible_adjust(id, SATURATION, &mut v.s, (0.0, 1.0), 0.05);
        ui.accessible_adjust(id, SQUARE, &mut v.v, (0.0, 1.0), 0.05);
        ui.accessible_adjust(id, HUE, &mut v.h, (0.0, 360.0), 18.0);
        ui.accessible_adjust(id, ALPHA, &mut v.a, (0.0, 1.0), 0.05);
        let v = *v;
        let mut slider = |part, label, value, span: f32| {
            ui.accessible_range(id, part, Role::Slider, value, (0.0, span), span / 20.0);
            ui.accessible_label(id, part, label);
        };
        slider(SATURATION, "Saturation", v.s, 1.0);
        slider(SQUARE, "Brightness", v.v, 1.0);
        slider(HUE, "Hue", v.h, 360.0);
        if self.alpha {
            slider(ALPHA, "Opacity", v.a, 1.0);
        }
        ui.with_accessible(id, HUE, |node| {
            node.set_value(format!("{:.0} degrees", v.h));
        });
        if let Some(hex) = hex {
            ui.accessible_description(id, HEX, Role::TextInput, "Hex", Some(hex));
        }
    }
}

/// Where the handle and the strip marks sit for `value` in the given parts.
fn marks(value: Hsv, square: Rect, hue: Rect, alpha: Option<Rect>) -> (Point, Rect, Option<Rect>) {
    let spot = Point::new(
        square.x + value.s * square.width,
        square.y + (1.0 - value.v) * square.height,
    );
    (
        spot,
        strip_mark(hue, value.h / 360.0),
        alpha.map(|strip| strip_mark(strip, 1.0 - value.a)),
    )
}

impl Look<PickerParts<'_>> {
    /// The hex field, drawn, and typed into: it edits the colour, so the square and strips after
    /// it show the colour typed this frame. Does nothing when no hex field is shown.
    pub fn paint_hex<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id) -> AureaResult<()> {
        let parts = &mut self.parts;
        let (Some(field), Some(text)) = (parts.hex, parts.hex_text.as_deref_mut()) else {
            return Ok(());
        };
        let before = parts.value;
        hex(
            ui,
            (id, HEX),
            parts.hex_input,
            field,
            text,
            &mut parts.value,
            &parts.style,
        )?;
        if parts.value != before {
            (parts.spot, parts.hue_mark, parts.alpha_mark) =
                marks(parts.value, parts.square, parts.hue, parts.alpha);
            self.response.changed = true;
            ui.mark_changed();
        }
        Ok(())
    }

    /// The picker's own look: the square, the hue strip, the alpha strip if shown, then the focus.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_field(ui)?;
        self.paint_hue(ui)?;
        self.paint_alpha(ui)?;
        self.paint_focus(ui)
    }

    /// The saturation and value square, its outline and its handle.
    pub fn paint_field<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let parts = &self.parts;
        SvField { hue: parts.value.h }.paint(ui, parts.square)?;
        ui.border(
            parts.square,
            0.0,
            1.0,
            parts.style.border.fade(self.state.enabled),
        )?;
        PickRing {
            at: parts.spot,
            radius: parts.style.ring,
        }
        .paint(ui, parts.square)
    }

    /// The hue strip with its mark.
    pub fn paint_hue<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let parts = &self.parts;
        paint_strip(
            ui,
            parts.hue_state,
            parts.hue,
            parts.hue_mark,
            parts.style,
            |ui, rect| HueStrip { vertical: true }.paint(ui, rect),
        )
    }

    /// The alpha strip with its mark, when it is shown.
    pub fn paint_alpha<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let parts = &self.parts;
        let (Some(rect), Some(state), Some(mark)) =
            (parts.alpha, parts.alpha_state, parts.alpha_mark)
        else {
            return Ok(());
        };
        let color = parts.value.to_color();
        let checker = parts.style.checker;
        paint_strip(ui, state, rect, mark, parts.style, |ui, rect| {
            AlphaStrip {
                color,
                checker,
                vertical: true,
            }
            .paint(ui, rect)
        })
    }

    /// The focus ring round the square, when focus came from the keyboard.
    pub fn paint_focus<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        FocusRing::new(ui.theme, 0.0, self.state.focus).paint(ui, self.parts.square)
    }
}

impl<Id: WidgetId> Widget<Id> for ColorPicker<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let mut look = self.look(ui, id, rect);
        look.paint_hex(ui, id)?;
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

pub(super) fn merge(response: &mut Response, input: Interaction) {
    response.hovered |= input.hovered;
    response.held |= input.held;
    response.focused |= input.focused;
}

pub(super) fn step<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    if ui.modifiers().shift { 0.1 } else { 0.01 }
}

pub(super) fn follow_square<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    input: Interaction,
    rect: Rect,
    value: &mut Hsv,
) {
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

/// The mark on a strip at `t` of its length, from the top.
fn strip_mark(rect: Rect, t: f32) -> Rect {
    let y = rect.y + t.clamp(0.0, 1.0) * rect.height;
    Rect::new(rect.x - 2.0, y - 2.0, rect.width + 4.0, 4.0)
}

fn paint_strip<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    state: Eased,
    rect: Rect,
    mark: Rect,
    style: PickerStyle,
    fill: impl FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<()>,
) -> AureaResult<()> {
    FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
    fill(ui, rect)?;
    ui.border(rect, 0.0, 1.0, style.border.fade(state.enabled))?;
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn look_paints_the_apps_look_and_still_picks() {
        let mut rig = Rig::new(240, 220).still();
        let (mut value, mut hex) = (Hsv::new(120.0, 0.5, 0.5, 1.0), String::new());
        let show = |rig: &mut Rig, value: &mut Hsv, hex: &mut String| {
            rig.frame(|ui| {
                let mut picker = ColorPicker::new(value)
                    .alpha(true)
                    .hex_field(hex)
                    .look(ui, 0, RECT);
                picker.paint_hex(ui, 0).expect("hex");
                ui.fill(picker.parts.hue, Color::rgb(0, 200, 0))
                    .expect("draw");
                picker.response
            })
        };
        show(&mut rig, &mut value, &mut hex);
        let hue = Picker::new().layout().hue;
        assert_eq!(
            rig.pixel((hue.x + 8.0) as u32, (hue.y + 2.0) as u32),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        let square = Picker::new().layout().square;
        rig.at(square.right(), square.y);
        rig.button(square.right(), square.y, true);
        show(&mut rig, &mut value, &mut hex);
        assert_eq!((value.s, value.v), (1.0, 1.0));
        assert_eq!(value.h, 120.0, "the hue stays");
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
