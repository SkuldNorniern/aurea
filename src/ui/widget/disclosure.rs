//! A disclosure: a header that opens and closes the section under it, its chevron turning from
//! right to down.

use std::f32::consts::FRAC_PI_2;

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a disclosure header looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisclosureStyle {
    /// Fill of the header, by state.
    pub face: StateColors,
    /// Colour of the title.
    pub text: Color,
    /// Colour of the chevron.
    pub chevron: Color,
    /// Corner radius in pixels.
    pub radius: f32,
    /// Size of the title text.
    pub text_size: TextSize,
}

impl DisclosureStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            face: StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
            text: p.text,
            chevron: p.dim,
            radius: theme.radius.min(6.0),
            text_size: TextSize::Strong,
        }
    }
}

/// A header titled `title`; clicking it, or Enter or Space, flips `open`.
#[derive(Debug)]
pub struct Disclosure<'v> {
    title: &'v str,
    open: &'v mut bool,
    style: Option<DisclosureStyle>,
}

impl<'v> Disclosure<'v> {
    /// Creates a disclosure titled `title` driven by `open`.
    pub fn new(title: &'v str, open: &'v mut bool) -> Self {
        Self {
            title,
            open,
            style: None,
        }
    }

    /// Sets the style.
    #[must_use]
    pub fn style(mut self, style: DisclosureStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Shows the header as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    /// How far open `id`'s section is this frame, `0..=1`, eased.
    pub fn openness<Id: WidgetId>(ui: &Ui<'_, Id>, id: Id) -> f32 {
        ui.state
            .tweens
            .get((Key::new(id, 0), Channel::Value))
            .unwrap_or(0.0)
    }
}

/// What a [`Disclosure`] draws, for a painter given to [`Disclosure::show_with`].
#[derive(Clone, Copy, Debug)]
pub struct DisclosureParts<'v> {
    /// The header's title.
    pub title: &'v str,
    /// Whether it is open, after this frame's click.
    pub open: bool,
    /// How far open the section is, `0..=1`, eased.
    pub openness: f32,
    /// The style it draws with, the theme's unless one was given.
    pub style: DisclosureStyle,
}

impl<'v> Disclosure<'v> {
    /// Takes the input and works out what it would draw, without drawing.
    fn run<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> (Response, Look<DisclosureParts<'v>>) {
        let input = ui.interact(id, 0, rect, Sense::Click, true);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.open = !*self.open;
            response.changed = true;
            ui.mark_changed();
        }
        #[cfg(feature = "accessibility")]
        {
            let open = *self.open;
            ui.accessible_description(id, 0, Role::DisclosureTriangle, self.title, None);
            ui.with_accessible(id, 0, |node| node.set_expanded(open));
        }
        let style = self.style.unwrap_or_else(|| DisclosureStyle::new(ui.theme));
        let state = ui.eased(id, 0, input);
        let t = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(*self.open)),
            ui.theme.motion.slide,
        );
        let look = Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            parts: DisclosureParts {
                title: self.title,
                open: *self.open,
                openness: t,
                style,
            },
        };
        (response, look)
    }

    /// Takes the input as [`Disclosure::show`] does, then has `paint` draw it instead of its own
    /// look. [`Look::paint_default`], [`Look::paint_face`], [`Look::paint_chevron`] and
    /// [`Look::paint_title`] draw the parts of the usual look.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        paint: impl FnOnce(&mut Ui<'_, Id>, &Look<DisclosureParts<'v>>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let (response, look) = self.run(ui, id, rect);
        paint(ui, &look)?;
        Ok(response)
    }
}

impl Look<DisclosureParts<'_>> {
    /// The header's own look: the face, the chevron, then the title.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_face(ui)?;
        self.paint_chevron(ui)?;
        self.paint_title(ui)
    }

    /// The header's face, with its focus ring.
    pub fn paint_face<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .paint(ui, self.rect)?;
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, self.rect)
    }

    /// The chevron, turned by how far open the section is.
    pub fn paint_chevron<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (mark, _) = self.split();
        chevron(
            ui,
            mark.inset(self.rect.height * 0.32),
            self.parts.openness,
            self.parts.style.chevron,
        )
    }

    /// The title, right of the chevron.
    pub fn paint_title<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (_, text) = self.split();
        Label::new(self.parts.title, self.parts.style.text)
            .size(self.parts.style.text_size)
            .paint(ui, text)
    }

    /// The square the chevron sits in, and the rect the title is left in, from the header.
    pub fn split(&self) -> (Rect, Rect) {
        self.rect.split_left(self.rect.height, 0.0)
    }
}

impl<Id: WidgetId> Widget<Id> for Disclosure<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_with(ui, id, rect, |ui, look| look.paint_default(ui))
    }
}

fn chevron<Id: WidgetId>(ui: &mut Ui<'_, Id>, at: Rect, t: f32, color: Color) -> AureaResult<()> {
    let c = at.center();
    let s = at.width.min(at.height) / 2.0;
    let (sin, cos) = (t * FRAC_PI_2).sin_cos();
    let turn =
        |x: f32, y: f32| Point::new(c.x + (x * cos - y * sin) * s, c.y + (x * sin + y * cos) * s);
    let points = [turn(-0.3, -0.6), turn(0.3, 0.0), turn(-0.3, 0.6)];
    ui.polyline(&points, (s * 0.28).max(1.2), color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::rig::Rig;
    use crate::window::{Modifiers, MouseButton, PointerKind, WindowEvent};

    #[test]
    fn a_click_opens_it_and_the_section_follows() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(200, 40);
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut open = false;
        let mut shown = 0.0;
        for event in [None, Some(true), Some(false)] {
            if let Some(pressed) = event {
                state.handle(&WindowEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed,
                    modifiers: Modifiers::default(),
                    x: 50.0,
                    y: 15.0,
                    click_count: 1,
                    time: Default::default(),
                    pointer: PointerKind::Mouse,
                });
            }
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                Disclosure::new("Advanced", &mut open)
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 200.0, 30.0))
                    .expect("draw");
                shown = Disclosure::openness(&ui, 1);
            }
            drop(ctx);
            raster.end_frame().expect("end");
        }
        assert!(open);
        assert_eq!(shown, 1.0, "no motion asked for, so open at once");
    }

    #[test]
    fn show_with_draws_the_apps_look_and_still_opens() {
        let mut rig = Rig::new(200, 30).still();
        let at = Rect::new(0.0, 0.0, 200.0, 30.0);
        let mut open = false;
        let draw = |rig: &mut Rig, open: &mut bool| {
            rig.frame(|ui| {
                Disclosure::new("Advanced", open)
                    .show_with(ui, 1, at, |ui, look| {
                        ui.fill(look.rect, Color::rgb(0, 200, 0))
                    })
                    .expect("draw");
            });
        };
        draw(&mut rig, &mut open);
        assert_eq!(
            rig.pixel(100, 15),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        rig.at(50, 15);
        draw(&mut rig, &mut open);
        rig.button(50, 15, true);
        draw(&mut rig, &mut open);
        rig.button(50, 15, false);
        draw(&mut rig, &mut open);
        assert!(open, "the click still opens it");
    }
}
