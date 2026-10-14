//! A dropdown: a [`Surface`] showing the picked option and a chevron, and
//! the popup list of the options when clicked.

use super::Widget;
use super::popup::{MenuItem, PopupStyle};
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How a dropdown looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropdownStyle {
    pub popup: PopupStyle,
    pub face: StateColors,
    pub border: Color,
    pub open_border: Color,
    pub text: Color,
    pub placeholder: Color,
    pub chevron: Color,
    pub radius: f32,
    pub padding: f32,
}

impl DropdownStyle {
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

/// Picks one of `options`. With the keyboard, Up and Down change the pick
/// while it is closed, as a native one does; Enter or Space opens the list.
#[derive(Debug)]
pub struct Dropdown<'v> {
    options: &'v [&'v str],
    selected: &'v mut usize,
    placeholder: &'v str,
    style: Option<DropdownStyle>,
    enabled: bool,
}

impl<'v> Dropdown<'v> {
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
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

    #[must_use]
    pub fn style(mut self, style: DropdownStyle) -> Self {
        self.style = Some(style);
        self
    }

    #[must_use]
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

    /// Up and Down move the pick while it is closed.
    fn keys<Id: WidgetId>(&mut self, ui: &Ui<'_, Id>) -> bool {
        let n = self.options.len();
        let before = *self.selected;
        for (key, _) in ui.keys() {
            *self.selected = match key {
                KeyCode::Down => (*self.selected + 1).min(n - 1),
                KeyCode::Up => self.selected.saturating_sub(1).min(n - 1),
                KeyCode::Home => 0,
                KeyCode::End => n - 1,
                _ => continue,
            };
        }
        *self.selected != before
    }
}

impl<Id: WidgetId> Widget<Id> for Dropdown<'_> {
    fn show(mut self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
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
        let open = ui.state.popup.is_open_for(key);
        if !self.options.is_empty() {
            if input.clicked && !open {
                let current = *self.selected;
                let items = self
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, label)| MenuItem::new(label).checked(i == current));
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
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .border(
                ui.theme.border_width,
                style.border.mix(style.open_border, lit).fade(state.enabled),
            )
            .paint(ui, rect)?;
        let chevron = rect.split_right(rect.height, 0.0).0;
        let shape = if lit > 0.5 {
            MarkShape::ChevronUp
        } else {
            MarkShape::ChevronDown
        };
        Mark::new(shape, style.chevron.fade(state.enabled))
            .paint(ui, chevron.inset(rect.height * 0.28))?;
        let text = Rect::new(
            rect.x + style.padding,
            rect.y,
            (chevron.x - rect.x - style.padding).max(0.0),
            rect.height,
        );
        match self.options.get(*self.selected) {
            Some(label) => Label::new(label, style.text.fade(state.enabled)).paint(ui, text)?,
            None => Label::new(self.placeholder, style.placeholder.fade(state.enabled))
                .paint(ui, text)?,
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use crate::Rect;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Dropdown, MenuItem, Sense, Theme, UiState};
    use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, WindowEvent};

    const BOX: Rect = Rect::new(10.0, 10.0, 160.0, 30.0);
    const AREA: Rect = Rect::new(200.0, 10.0, 100.0, 100.0);
    const OPTIONS: [&str; 3] = ["Small", "Medium", "Large"];

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        picked: usize,
        changed: u32,
        menu: Option<usize>,
    }

    impl Rig {
        fn new() -> Self {
            let mut ui = UiState::new();
            ui.set_reduced_motion(true);
            let mut rig = Self {
                raster: CpuRasterizer::new(320, 260),
                ui,
                theme: Theme::dark(),
                picked: 0,
                changed: 0,
                menu: None,
            };
            rig.frame();
            rig
        }

        fn frame(&mut self) {
            let mut ctx = self.raster.begin_frame().expect("frame");
            {
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                let r = Dropdown::new(&OPTIONS, &mut self.picked)
                    .show(&mut ui, 1, BOX)
                    .expect("draw");
                self.changed += u32::from(r.changed);
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
                    self.menu = Some(row);
                }
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
        }

        fn mouse(&mut self, button: MouseButton, x: f32, y: f32, pressed: bool) {
            self.ui.handle(&WindowEvent::MouseMove {
                x: f64::from(x),
                y: f64::from(y),
                buttons: MouseButtons::default(),
                modifiers: Modifiers::default(),
                time: Default::default(),
            });
            self.ui.handle(&WindowEvent::MouseButton {
                button,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 1,
                time: Default::default(),
            });
            self.frame();
        }

        fn click(&mut self, x: f32, y: f32) {
            self.mouse(MouseButton::Left, x, y, true);
            self.mouse(MouseButton::Left, x, y, false);
            self.frame();
        }

        fn key(&mut self, key: KeyCode) {
            self.ui.handle(&WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            self.frame();
        }

        /// The middle of option `i`'s row, under the box.
        fn row(i: usize) -> (f32, f32) {
            #[allow(clippy::cast_precision_loss)]
            let i = i as f32;
            let row = (Theme::dark().control_height * 0.92).round();
            (BOX.x + 40.0, BOX.bottom() + 4.0 + 4.0 + row * (i + 0.5))
        }
    }

    #[test]
    fn clicking_an_option_picks_it_and_closes() {
        let mut rig = Rig::new();
        rig.click(40.0, 25.0);
        let (x, y) = Rig::row(2);
        rig.click(x, y);
        assert_eq!(rig.picked, 2);
        assert_eq!(rig.changed, 1);
        rig.click(40.0, 25.0);
        let (x, y) = Rig::row(2);
        rig.click(x, y);
        assert_eq!(rig.changed, 1, "picking what was picked changes nothing");
    }

    #[test]
    fn a_click_outside_closes_without_reaching_what_is_under() {
        let mut rig = Rig::new();
        rig.click(40.0, 25.0);
        rig.mouse(MouseButton::Right, 250.0, 50.0, true);
        assert_eq!(rig.menu, None);
        rig.click(40.0, 25.0);
        rig.click(300.0, 240.0);
        let (x, y) = Rig::row(1);
        rig.click(x, y);
        assert_eq!(rig.picked, 0, "the list had closed");
    }

    #[test]
    fn the_keyboard_works_the_list() {
        let mut rig = Rig::new();
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
        assert!(rig.ui.has_focus(), "and the keyboard stays on the dropdown");
    }

    #[test]
    fn a_context_menu_opens_where_asked_and_skips_what_is_off() {
        let mut rig = Rig::new();
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
}
