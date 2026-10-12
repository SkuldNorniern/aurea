//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every platform.

pub mod component;
mod frame;
pub mod paint;
mod state;
pub mod text_edit;
mod theme;
pub mod widget;

pub use component::{
    EditableText, FocusRing, Knob, PanZoom, StateColors, TextOutcome, Track, card,
};
pub use frame::{Align, Eased, PressAt, Response, Ui};
pub use paint::{Border, Component, Label, Mark, MarkShape, Overflow, Shadow, Surface, TextSize};
pub use state::{Channel, Fonts, Interaction, Sense, UiState, WidgetId};
pub use theme::{Motion, Palette, Theme};
pub use widget::Widget;
pub use widget::button::{Button, ButtonStyle, Tone};
pub use widget::progress::{ProgressBar, ProgressStyle};
pub use widget::segmented::{Segmented, SegmentedStyle};
pub use widget::slider::{Slider, SliderStyle};
pub use widget::text_field::{FieldStyle, TextField};
pub use widget::toggle::{Checkbox, CheckboxStyle, Toggle, ToggleStyle};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, WindowEvent};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum Id {
        Go,
        Other,
        Switch,
        Name,
        Level,
    }

    #[derive(Default)]
    struct App {
        clicks: u32,
        on: bool,
        name: String,
        level: f32,
    }

    const GO: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);
    const OTHER: Rect = Rect::new(120.0, 10.0, 100.0, 30.0);
    const SWITCH: Rect = Rect::new(10.0, 50.0, 200.0, 28.0);
    const NAME: Rect = Rect::new(10.0, 90.0, 200.0, 30.0);
    const LEVEL: Rect = Rect::new(10.0, 130.0, 200.0, 24.0);

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<Id>,
        theme: Theme,
        app: App,
    }

    impl Rig {
        fn new() -> Self {
            let mut ui = UiState::new();
            ui.set_reduced_motion(true);
            let mut rig = Self {
                raster: CpuRasterizer::new(240, 170),
                ui,
                theme: Theme::dark(),
                app: App::default(),
            };
            rig.frame();
            rig
        }

        fn frame(&mut self) {
            let mut ctx = self.raster.begin_frame().expect("frame");
            {
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                let app = &mut self.app;
                if Button::new("Go")
                    .show(&mut ui, Id::Go, GO)
                    .expect("draw")
                    .clicked
                {
                    app.clicks += 1;
                }
                Button::new("Other")
                    .show(&mut ui, Id::Other, OTHER)
                    .expect("draw");
                Toggle::new(&mut app.on)
                    .label("On")
                    .show(&mut ui, Id::Switch, SWITCH)
                    .expect("draw");
                TextField::new(&mut app.name)
                    .show(&mut ui, Id::Name, NAME)
                    .expect("draw");
                Slider::new(&mut app.level, 0.0..=1.0)
                    .step(0.1)
                    .show(&mut ui, Id::Level, LEVEL)
                    .expect("draw");
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
        }

        fn send(&mut self, event: WindowEvent) -> bool {
            self.ui.handle(&event)
        }

        fn button(&mut self, x: f32, y: f32, pressed: bool) {
            self.send(WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 1,
            });
        }

        fn move_to(&mut self, x: f32, y: f32) -> bool {
            self.send(WindowEvent::MouseMove {
                x: f64::from(x),
                y: f64::from(y),
                buttons: MouseButtons::default(),
                modifiers: Modifiers::default(),
            })
        }

        fn click(&mut self, rect: Rect) {
            let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            self.button(x, y, true);
            self.button(x, y, false);
            self.frame();
        }

        fn key(&mut self, key: KeyCode, modifiers: Modifiers) {
            self.send(WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers,
            });
            self.frame();
        }
    }

    #[test]
    fn a_click_lands_on_what_was_drawn_there() {
        let mut rig = Rig::new();
        rig.click(GO);
        assert_eq!(rig.app.clicks, 1);
        rig.click(OTHER);
        assert_eq!(rig.app.clicks, 1);
    }

    #[test]
    fn letting_go_somewhere_else_is_no_click() {
        let mut rig = Rig::new();
        rig.button(20.0, 20.0, true);
        rig.frame();
        rig.button(200.0, 160.0, false);
        rig.frame();
        assert_eq!(rig.app.clicks, 0);
    }

    #[test]
    fn a_switch_flips_on_click() {
        let mut rig = Rig::new();
        rig.click(SWITCH);
        assert!(rig.app.on);
        rig.click(SWITCH);
        assert!(!rig.app.on);
    }

    #[test]
    fn tab_walks_the_widgets_and_space_presses() {
        let mut rig = Rig::new();
        rig.key(KeyCode::Tab, Modifiers::default());
        rig.key(KeyCode::Space, Modifiers::default());
        assert_eq!(rig.app.clicks, 1, "Tab lands on the first button");
        rig.key(KeyCode::Tab, Modifiers::default());
        rig.key(KeyCode::Tab, Modifiers::default());
        rig.key(KeyCode::Enter, Modifiers::default());
        assert!(rig.app.on, "two more Tabs reach the switch");
        let back = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        rig.key(KeyCode::Tab, back);
        rig.key(KeyCode::Tab, back);
        rig.key(KeyCode::Enter, Modifiers::default());
        assert_eq!(rig.app.clicks, 2, "Shift+Tab goes back");
    }

    #[test]
    fn a_focused_field_takes_typing_and_the_rest_do_not() {
        let mut rig = Rig::new();
        assert!(!rig.send(WindowEvent::TextInput { text: "x".into() }));
        rig.click(NAME);
        assert!(rig.ui.text_focused());
        assert!(rig.send(WindowEvent::TextInput {
            text: "hello".into()
        }));
        rig.frame();
        rig.key(KeyCode::Backspace, Modifiers::default());
        assert_eq!(rig.app.name, "hell");
        assert!(
            rig.ui.ime_area().is_some(),
            "the input method is told where the caret is"
        );
        rig.key(KeyCode::Escape, Modifiers::default());
        assert!(!rig.ui.has_focus());
    }

    #[test]
    fn dragging_a_slider_and_its_keys() {
        let mut rig = Rig::new();
        rig.button(LEVEL.x + 2.0, LEVEL.y + 12.0, true);
        rig.frame();
        rig.move_to(LEVEL.right(), LEVEL.y + 12.0);
        rig.frame();
        assert_eq!(rig.app.level, 1.0);
        rig.button(LEVEL.right(), LEVEL.y + 12.0, false);
        rig.frame();
        rig.key(KeyCode::Left, Modifiers::default());
        assert!(
            (rig.app.level - 0.9).abs() < 1e-4,
            "the step, from the keyboard"
        );
    }

    #[test]
    fn an_idle_ui_asks_for_no_frames() {
        let mut rig = Rig::new();
        rig.frame();
        assert!(!rig.ui.wants_frame());
        assert!(
            !rig.move_to(230.0, 165.0),
            "moving over nothing needs no frame"
        );
        assert!(rig.move_to(20.0, 20.0), "moving onto a button does");
    }
}
