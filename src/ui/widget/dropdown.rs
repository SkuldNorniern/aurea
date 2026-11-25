//! A dropdown: a [`Surface`] showing the picked option and a chevron, and the popup list of the
//! options when clicked.

use super::Widget;
use super::adorn::{Adornment, Leading, paint_leading};
use super::look::Look;
use super::popup::{MenuItem, PopupStyle};
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Badge, Component, IconSource, Label, Mark, MarkShape, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How a dropdown looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropdownStyle {
    /// Style of the popup list.
    pub popup: PopupStyle,
    /// Fill colours by state.
    pub face: StateColors,
    /// Border colour.
    pub border: Color,
    /// Border colour while open.
    pub open_border: Color,
    /// Selected text colour.
    pub text: Color,
    /// Placeholder text colour.
    pub placeholder: Color,
    /// Chevron colour.
    pub chevron: Color,
    /// Corner radius.
    pub radius: f32,
    /// Horizontal inset of the text.
    pub padding: f32,
}

impl DropdownStyle {
    /// A dropdown style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            popup: PopupStyle::new(theme),
            face: StateColors::new(p.raised, p.hover, p.pressed),
            border: p.border,
            open_border: p.accent,
            text: p.text,
            placeholder: p.faint,
            chevron: p.dim,
            radius: theme.radius,
            padding: theme.padding,
        }
    }
}

/// One option of a [`Dropdown`] that has a mark, a badge, or can't be picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropdownOption<'a> {
    /// The option's text.
    pub label: &'a str,
    /// An icon or colour in front and a badge after the text.
    pub adornment: Adornment<'a>,
    /// Whether it can be picked.
    pub enabled: bool,
}

impl<'a> DropdownOption<'a> {
    /// An option showing `label`.
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            adornment: Adornment::NONE,
            enabled: true,
        }
    }

    /// Sets an icon in front, in the text colour. Replaces a swatch.
    #[must_use]
    pub fn icon(mut self, icon: impl Into<IconSource<'a>>) -> Self {
        self.adornment = self.adornment.icon(icon);
        self
    }

    /// Sets a coloured dot in front. Replaces an icon.
    #[must_use]
    pub const fn swatch(mut self, color: Color) -> Self {
        self.adornment = self.adornment.swatch(color);
        self
    }

    /// Sets a badge after the text, such as a count.
    #[must_use]
    pub const fn badge(mut self, badge: Badge<'a>) -> Self {
        self.adornment = self.adornment.badge(badge);
        self
    }

    /// Sets whether it can be picked.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn row(self, checked: bool) -> MenuItem<'a> {
        MenuItem {
            adornment: self.adornment,
            enabled: self.enabled,
            checked,
            ..MenuItem::new(self.label)
        }
    }
}

impl<'a> From<&'a str> for DropdownOption<'a> {
    fn from(label: &'a str) -> Self {
        Self::new(label)
    }
}

/// The options as given: plain labels, or rows with marks and states.
#[derive(Clone, Copy, Debug)]
enum Options<'v> {
    Labels(&'v [&'v str]),
    Rows(&'v [DropdownOption<'v>]),
}

impl<'v> Options<'v> {
    fn len(self) -> usize {
        match self {
            Self::Labels(labels) => labels.len(),
            Self::Rows(rows) => rows.len(),
        }
    }

    fn is_empty(self) -> bool {
        self.len() == 0
    }

    fn get(self, i: usize) -> Option<DropdownOption<'v>> {
        match self {
            Self::Labels(labels) => labels.get(i).copied().map(DropdownOption::new),
            Self::Rows(rows) => rows.get(i).copied(),
        }
    }
}

/// Picks one of `options`.
#[derive(Debug)]
pub struct Dropdown<'v> {
    options: Options<'v>,
    selected: &'v mut usize,
    placeholder: &'v str,
    style: Option<DropdownStyle>,
    enabled: bool,
}

impl<'v> Dropdown<'v> {
    /// A dropdown showing `selected` of `options`.
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self::from_options(Options::Labels(options), selected)
    }

    /// A dropdown of [`DropdownOption`]s: marks, badges, and options that can't be picked.
    pub fn with_options(options: &'v [DropdownOption<'v>], selected: &'v mut usize) -> Self {
        Self::from_options(Options::Rows(options), selected)
    }

    fn from_options(options: Options<'v>, selected: &'v mut usize) -> Self {
        Self {
            options,
            selected,
            placeholder: "",
            style: None,
            enabled: true,
        }
    }

    /// Shown while `selected` is past the options.
    #[must_use]
    pub fn placeholder(mut self, placeholder: &'v str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Sets the look.
    #[must_use]
    pub fn style(mut self, style: DropdownStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether it takes input.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws it in `rect` and reports the input.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    fn enabled_at(&self, i: usize) -> bool {
        self.options.get(i).is_some_and(|option| option.enabled)
    }

    /// The nearest enabled option from `from` in one direction, or `from` when there is none.
    fn step(&self, from: usize, forward: bool) -> usize {
        let n = self.options.len();
        let mut at = from;
        loop {
            at = match (forward, at) {
                (true, at) if at + 1 < n => at + 1,
                (false, at) if at > 0 => at - 1,
                _ => return from,
            };
            if self.enabled_at(at) {
                return at;
            }
        }
    }

    /// The first (or last) enabled option, or `from` when every option is off.
    fn end(&self, from: usize, first: bool) -> usize {
        let n = self.options.len();
        let picked = if first {
            (0..n).find(|&i| self.enabled_at(i))
        } else {
            (0..n).rev().find(|&i| self.enabled_at(i))
        };
        picked.unwrap_or(from)
    }

    fn keys<Id: WidgetId>(&mut self, ui: &Ui<'_, Id>) -> bool {
        let before = *self.selected;
        for (key, _) in ui.keys() {
            let at = *self.selected;
            *self.selected = match key {
                KeyCode::Down => self.step(at, true),
                KeyCode::Up => self.step(at, false),
                KeyCode::Home => self.end(at, true),
                KeyCode::End => self.end(at, false),
                _ => continue,
            };
        }
        *self.selected != before
    }
}

/// What a [`Dropdown`] draws, from [`Dropdown::look`].
#[derive(Clone, Copy, Debug)]
pub struct DropdownParts<'v> {
    /// The picked option's text, after this frame's keys and pick; `None` past the options.
    pub value: Option<&'v str>,
    /// The picked option's icon or colour, `None` when it has none.
    pub leading: Leading<'v>,
    /// Shown while there is no value.
    pub placeholder: &'v str,
    /// How far the popup list is open, 0 shut to 1 open, eased.
    pub opened: f32,
    /// The style it draws with, the theme's unless one was given.
    pub style: DropdownStyle,
}

impl<'v> Dropdown<'v> {
    /// Takes the input as [`Dropdown::show`] does and works out what it would draw, without
    /// drawing: the app draws it, or [`Look::paint_default`] does, or its pieces
    /// [`Look::paint_field`], [`Look::paint_chevron`] and [`Look::paint_value`].
    pub fn look<Id: WidgetId>(
        mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<DropdownParts<'v>> {
        let key = Key::new(id, 0);
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        let style = self.style.unwrap_or_else(|| DropdownStyle::new(ui.theme));
        if let Some(picked) = ui.state.popup.take_pick(key)
            && picked != *self.selected
        {
            *self.selected = picked;
            response.changed = true;
        }
        // no label here, so one from `accessible_label` stays.
        {
            let value = self.options.get(*self.selected).map(|option| option.label);
            ui.with_accessible(id, 0, |node| {
                node.set_role(Role::ComboBox);
                node.set_value(value.unwrap_or(self.placeholder));
            });
        }
        let open = ui.state.popup.is_open_for(key);
        if !self.options.is_empty() {
            if input.clicked && !open {
                let current = *self.selected;
                let options = self.options;
                let items = (0..options.len())
                    .filter_map(|i| options.get(i).map(|option| option.row(i == current)));
                ui.state.popup.open(key, rect, rect.width, items, Some(key));
                ui.state.popup.style = Some(style.popup);
            } else if input.focused && !open && self.keys(ui) {
                response.changed = true;
            }
        }
        if ui.state.popup.is_open_for(key) {
            ui.state.popup.style = Some(style.popup);
        }
        if response.changed {
            ui.mark_changed();
        }

        let state = ui.eased(id, 0, input);
        let lit = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(open)),
            ui.theme.motion.focus,
        );
        let picked = self.options.get(*self.selected);
        Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            response,
            parts: DropdownParts {
                value: picked.map(|option| option.label),
                leading: picked.map_or(Leading::None, |option| option.adornment.leading),
                placeholder: self.placeholder,
                opened: lit,
                style,
            },
        }
    }
}

impl Look<DropdownParts<'_>> {
    /// The closed control's own look: the field, the chevron, then the value.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_field(ui)?;
        self.paint_chevron(ui)?;
        self.paint_value(ui)
    }

    /// The field: its focus ring, face and border, the border lit towards the open one as the list
    /// opens.
    pub fn paint_field<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        let lit = self.parts.opened;
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, self.rect)?;
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .border(
                ui.theme.border_width,
                style.border.mix(style.open_border, lit).fade(state.enabled),
            )
            .paint(ui, self.rect)
    }

    /// The chevron at the field's end, up while the list is open.
    pub fn paint_chevron<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        let shape = if self.parts.opened > 0.5 {
            MarkShape::ChevronUp
        } else {
            MarkShape::ChevronDown
        };
        Mark::new(shape, style.chevron.fade(state.enabled))
            .paint(ui, self.chevron_rect().inset(self.rect.height * 0.28))
    }

    /// The picked option's mark, then its text, or the placeholder while there is none.
    pub fn paint_value<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        paint_leading(
            ui,
            self.leading_rect(),
            self.parts.leading,
            style.text,
            state.enabled,
        )?;
        let text = self.text_rect();
        match self.parts.value {
            Some(label) => Label::new(label, style.text.fade(state.enabled)).paint(ui, text),
            None => Label::new(
                self.parts.placeholder,
                style.placeholder.fade(state.enabled),
            )
            .paint(ui, text),
        }
    }

    /// The square at the field's end the chevron sits in.
    pub fn chevron_rect(&self) -> Rect {
        self.rect.split_right(self.rect.height, 0.0).0
    }

    /// The square at the field's start the picked option's mark sits in; zero wide when it has
    /// none, so the text keeps its place.
    pub fn leading_rect(&self) -> Rect {
        let width = match self.parts.leading {
            Leading::None => 0.0,
            Leading::Icon(_) | Leading::Swatch(_) => self.rect.height * 0.75,
        };
        Rect::new(
            self.rect.x + self.parts.style.padding,
            self.rect.y,
            width,
            self.rect.height,
        )
    }

    /// The rect the value is left in, between the mark and the chevron.
    pub fn text_rect(&self) -> Rect {
        let chevron = self.chevron_rect();
        let start = self.leading_rect().right();
        Rect::new(
            start,
            self.rect.y,
            (chevron.x - start).max(0.0),
            self.rect.height,
        )
    }
}

impl<Id: WidgetId> Widget<Id> for Dropdown<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use crate::Rect;
    use crate::render::{Color, Icon};
    use crate::ui::rig::Rig;
    use crate::ui::{Badge, Dropdown, DropdownOption, Leading, MenuItem, Sense, Theme};
    use crate::window::{KeyCode, Modifiers, MouseButton, PointerKind, WindowEvent};

    const BOX: Rect = Rect::new(10.0, 10.0, 160.0, 30.0);
    const AREA: Rect = Rect::new(200.0, 10.0, 100.0, 100.0);
    const OPTIONS: [&str; 3] = ["Small", "Medium", "Large"];

    struct Screen {
        rig: Rig,
        picked: usize,
        changed: u32,
        menu: Option<usize>,
    }

    impl Screen {
        fn new() -> Self {
            let mut screen = Self {
                rig: Rig::new(320, 260).still(),
                picked: 0,
                changed: 0,
                menu: None,
            };
            screen.frame();
            screen
        }

        fn frame(&mut self) {
            let (picked, changed, menu) = (&mut self.picked, &mut self.changed, &mut self.menu);
            self.rig.frame(|ui| {
                let r = Dropdown::new(&OPTIONS, picked)
                    .show(ui, 1, BOX)
                    .expect("draw");
                *changed += u32::from(r.changed);
                let area = ui.interact(2, 0, AREA, Sense::Hover, true);
                if area.secondary_clicked
                    && let Some(at) = ui.secondary_at(2, 0)
                {
                    let items = [
                        MenuItem::new("Cut").shortcut("Ctrl+X"),
                        MenuItem::separator(),
                        MenuItem::new("Gone").enabled(false),
                        MenuItem::new("Paste"),
                    ];
                    ui.open_menu(2, at, items);
                }
                if let Some(row) = ui.menu_picked(2) {
                    *menu = Some(row);
                }
            });
        }

        fn mouse(&mut self, button: MouseButton, x: f32, y: f32, pressed: bool) {
            self.rig.at(x, y);
            self.rig.send(WindowEvent::MouseButton {
                button,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 1,
                time: Default::default(),
                pointer: PointerKind::Mouse,
            });
            self.frame();
        }

        fn click(&mut self, x: f32, y: f32) {
            self.rig.at(x, y);
            self.rig.button(x, y, true);
            self.frame();
            self.rig.at(x, y);
            self.rig.button(x, y, false);
            self.frame();
            self.frame();
        }

        fn key(&mut self, key: KeyCode) {
            self.rig.key(key);
            self.frame();
        }

        fn row(i: usize) -> (f32, f32) {
            #[allow(clippy::cast_precision_loss)]
            let i = i as f32;
            let row = (Theme::dark().control_height * 0.92).round();
            (BOX.x + 40.0, BOX.bottom() + 4.0 + 4.0 + row * (i + 0.5))
        }
    }

    #[test]
    fn clicking_an_option_picks_it_and_closes() {
        let mut rig = Screen::new();
        rig.click(40.0, 25.0);
        let (x, y) = Screen::row(2);
        rig.click(x, y);
        assert_eq!(rig.picked, 2);
        assert_eq!(rig.changed, 1);
        rig.click(40.0, 25.0);
        let (x, y) = Screen::row(2);
        rig.click(x, y);
        assert_eq!(rig.changed, 1, "picking what was picked changes nothing");
    }

    #[test]
    fn a_click_outside_closes_without_reaching_what_is_under() {
        let mut rig = Screen::new();
        rig.click(40.0, 25.0);
        rig.mouse(MouseButton::Right, 250.0, 50.0, true);
        assert_eq!(rig.menu, None);
        rig.click(40.0, 25.0);
        rig.click(300.0, 240.0);
        let (x, y) = Screen::row(1);
        rig.click(x, y);
        assert_eq!(rig.picked, 0, "the list had closed");
    }

    #[test]
    fn the_keyboard_works_the_list() {
        let mut rig = Screen::new();
        rig.key(KeyCode::Tab);
        rig.key(KeyCode::Down);
        assert_eq!(rig.picked, 1, "closed, Down changes the pick");
        rig.key(KeyCode::Enter);
        rig.key(KeyCode::Down);
        rig.key(KeyCode::Enter);
        rig.frame();
        assert_eq!(rig.picked, 2);
        rig.key(KeyCode::Space);
        rig.key(KeyCode::Up);
        rig.key(KeyCode::Escape);
        rig.frame();
        assert_eq!(rig.picked, 2, "Escape leaves it");
        assert!(
            rig.rig.state.has_focus(),
            "and the keyboard stays on the dropdown"
        );
    }

    #[test]
    fn look_draws_the_apps_field_and_still_picks() {
        let mut rig = Rig::new(320, 260).still();
        let mut picked = 0;
        let show = |rig: &mut Rig, picked: &mut usize| {
            rig.frame(|ui| {
                let field = Dropdown::new(&OPTIONS, picked).look(ui, 1, BOX);
                ui.fill(field.rect, Color::rgb(0, 200, 0)).expect("draw");
                field.response
            })
        };
        show(&mut rig, &mut picked);
        assert_eq!(
            rig.pixel(100, 25),
            [0, 200, 0, 255],
            "the app's field, not the default"
        );
        rig.key(KeyCode::Tab);
        show(&mut rig, &mut picked);
        rig.key(KeyCode::Down);
        show(&mut rig, &mut picked);
        assert_eq!(picked, 1, "the keys still change the pick");
    }

    #[test]
    fn a_context_menu_opens_where_asked_and_skips_what_is_off() {
        let mut rig = Screen::new();
        rig.mouse(MouseButton::Right, 220.0, 30.0, true);
        rig.key(KeyCode::Down);
        rig.key(KeyCode::Down);
        rig.key(KeyCode::Enter);
        rig.frame();
        assert_eq!(
            rig.menu,
            Some(3),
            "the separator and the disabled row are passed over"
        );
    }

    #[test]
    fn a_string_list_keeps_its_text_column_and_has_no_mark() {
        let mut rig = Rig::new(320, 260).still();
        let mut picked = 1;
        let (plain, text) = rig.frame(|ui| {
            let field = Dropdown::new(&OPTIONS, &mut picked).look(ui, 1, BOX);
            (field.parts.leading == Leading::None, field.text_rect())
        });
        assert!(plain, "no mark");
        assert_eq!(
            text.x,
            BOX.x + Theme::dark().padding,
            "text where it always was"
        );
    }

    #[test]
    fn a_picked_swatch_is_drawn_in_the_field_before_the_text() {
        // red and blue read the same either way round, so the test holds on any byte order
        let violet = Color::rgb(200, 60, 200);
        let options = [
            DropdownOption::new("Red").swatch(Color::rgb(255, 0, 0)),
            DropdownOption::new("Violet").swatch(violet),
        ];
        let mut rig = Rig::new(320, 260).still();
        let mut picked = 1;
        let (slot, text) = rig.frame(|ui| {
            let field = Dropdown::with_options(&options, &mut picked).look(ui, 1, BOX);
            field.paint_default(ui).expect("draw");
            (field.leading_rect(), field.text_rect())
        });
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let center = slot.center();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let at = rig.pixel(center.x as u32, center.y as u32);
        assert_eq!(at, [200, 60, 200, 255], "the picked option's dot");
        assert!(text.x >= slot.right(), "the text starts after the dot");
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn a_picked_icon_is_drawn_in_the_text_colour() {
        let options = [DropdownOption::new("Add").icon(Icon::Plus)];
        let mut rig = Rig::new(320, 260).still();
        let mut picked = 0;
        let (slot, text) = rig.frame(|ui| {
            let field = Dropdown::with_options(&options, &mut picked).look(ui, 1, BOX);
            field.paint_default(ui).expect("draw");
            (field.leading_rect(), field.text_rect())
        });
        let center = slot.center();
        let inked = rig.pixel(center.x as u32, center.y as u32);
        assert!(
            inked[0] > 128,
            "the plus is drawn in the light text colour: {inked:?}"
        );
        assert!(text.x >= slot.right(), "the text starts after the icon");
    }

    #[test]
    fn disabled_options_are_skipped_by_the_keys() {
        let options = [
            DropdownOption::new("A"),
            DropdownOption::new("B").enabled(false),
            DropdownOption::new("C").badge(Badge::new("new")),
        ];
        let mut rig = Rig::new(320, 260).still();
        let mut picked = 0;
        let show = |rig: &mut Rig, picked: &mut usize| {
            rig.frame(|ui| {
                Dropdown::with_options(&options, picked)
                    .show(ui, 1, BOX)
                    .expect("draw");
            })
        };
        show(&mut rig, &mut picked);
        rig.key(KeyCode::Tab);
        show(&mut rig, &mut picked);
        rig.key(KeyCode::Down);
        show(&mut rig, &mut picked);
        assert_eq!(picked, 2, "B is off, so C");
    }
}
