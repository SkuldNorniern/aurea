//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every platform.

#[cfg(feature = "accessibility")]
pub mod accessibility;
pub mod component;
mod frame;
pub mod paint;
mod state;
pub mod text_edit;
mod theme;
pub mod widget;

pub use component::{
    EditableText, FocusRing, Knob, PanZoom, PlotView, StateColors, TextOutcome, Track, card,
};
pub use frame::{Align, Eased, PressAt, Response, TypedInput, Ui};
pub use paint::{
    AxisMarks, AxisSide, AxisStyle, Badge, BadgeTone, Border, Component, DetailRow, GridLines,
    GridStyle, IconMark, IconSource, Label, Mark, MarkShape, Marker, Overflow, PromptLine,
    Separator, Shadow, Spinner, Stroke, Surface, TextSize, Trace,
};
pub use state::{Channel, Fonts, InputResponse, Interaction, Sense, UiState, WidgetId};
pub use theme::{Motion, Palette, Theme};
pub use widget::Widget;
pub use widget::button::{Button, ButtonStyle, Tone};
pub use widget::chip::{Chip, ChipStyle};
pub use widget::color::{ColorPicker, PickerStyle};
pub use widget::disclosure::{Disclosure, DisclosureStyle};
pub use widget::dock::{
    Dock, DockAxis, DockBar, DockGroup, DockLayout, DockMetrics, DockPlan, DockSide, DockStyle,
    DockTabEvent, DockTree, DockView,
};
pub use widget::dropdown::{Dropdown, DropdownStyle};
pub use widget::empty::EmptyState;
pub use widget::graph;
pub use widget::graph::{GraphStyle, GraphView, ScopeStyle, ScopeView};
pub use widget::link::{Link, LinkStyle};
pub use widget::pager::{Pager, PagerStyle};
pub use widget::popup::{MenuItem, PopupStyle};
pub use widget::progress::{ProgressBar, ProgressStyle};
pub use widget::radio::{RadioGroup, RadioStyle};
pub use widget::scroll::{
    ListRow, ListView, ScrollArea, ScrollStyle, scroll_into_view, visible_rows,
};
pub use widget::segmented::{Segmented, SegmentedStyle};
pub use widget::slider::{Slider, SliderStyle, SliderTrack};
pub use widget::stepper::{Stepper, StepperStyle};
pub use widget::swatch::{SwatchStyle, Swatches};
pub use widget::tabs::{TabBar, TabStyle};
pub use widget::text_area::TextArea;
pub use widget::text_field::{FieldStyle, TextField};
pub use widget::toggle::{Checkbox, CheckboxStyle, Toggle, ToggleStyle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_on_either_side_of_tab_goes_to_its_own_field() {
        for reverse in [false, true] {
            let mut rig = Rig::new(160, 100);
            let (mut first, mut second) = (String::new(), String::new());
            let frame = |rig: &mut Rig, first: &mut String, second: &mut String, reverse: bool| {
                rig.frame(|ui| {
                    let a = Rect::new(0.0, 0.0, 120.0, 32.0);
                    let b = Rect::new(0.0, 40.0, 120.0, 32.0);
                    if reverse {
                        TextField::new(second).show(ui, 2, b).expect("second");
                        TextField::new(first).show(ui, 1, a).expect("first");
                    } else {
                        TextField::new(first).show(ui, 1, a).expect("first");
                        TextField::new(second).show(ui, 2, b).expect("second");
                    }
                });
            };
            frame(&mut rig, &mut first, &mut second, false);
            rig.key(KeyCode::Tab);
            frame(&mut rig, &mut first, &mut second, false);
            rig.type_text("a");
            rig.key(KeyCode::Tab);
            rig.type_text("b");
            frame(&mut rig, &mut first, &mut second, reverse);
            assert_eq!(first, "a");
            assert_eq!(second, "b");
            rig.type_text("c");
            frame(&mut rig, &mut first, &mut second, reverse);
            assert_eq!(second, "bc");
        }
    }
    use crate::Rect;
    use crate::ui::rig::{Rig, key_down, mouse_move};
    use crate::window::{KeyCode, Modifiers, WindowEvent};

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

    struct Screen {
        rig: Rig<Id>,
        app: App,
    }

    impl Screen {
        fn new() -> Self {
            let mut screen = Self {
                rig: Rig::new(240, 170).still(),
                app: App::default(),
            };
            screen.frame();
            screen
        }

        fn frame(&mut self) {
            let app = &mut self.app;
            self.rig.frame(|ui| {
                if Button::new("Go")
                    .show(ui, Id::Go, GO)
                    .expect("draw")
                    .clicked
                {
                    app.clicks += 1;
                }
                Button::new("Other")
                    .show(ui, Id::Other, OTHER)
                    .expect("draw");
                Toggle::new(&mut app.on)
                    .label("On")
                    .show(ui, Id::Switch, SWITCH)
                    .expect("draw");
                TextField::new(&mut app.name)
                    .show(ui, Id::Name, NAME)
                    .expect("draw");
                Slider::new(&mut app.level, 0.0..=1.0)
                    .step(0.1)
                    .show(ui, Id::Level, LEVEL)
                    .expect("draw");
            });
        }

        fn click(&mut self, rect: Rect) {
            let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            self.rig.click(x, y);
            self.frame();
        }

        fn key(&mut self, key: KeyCode, modifiers: Modifiers) {
            self.rig.send(key_down(key, modifiers));
            self.frame();
        }
    }

    #[test]
    fn a_click_lands_on_what_was_drawn_there() {
        let mut rig = Screen::new();
        rig.click(GO);
        assert_eq!(rig.app.clicks, 1);
        rig.click(OTHER);
        assert_eq!(rig.app.clicks, 1);
    }

    #[test]
    fn letting_go_somewhere_else_is_no_click() {
        let mut rig = Screen::new();
        rig.rig.button(20.0, 20.0, true);
        rig.frame();
        rig.rig.button(200.0, 160.0, false);
        rig.frame();
        assert_eq!(rig.app.clicks, 0);
    }

    #[test]
    fn losing_window_focus_cancels_queued_pointer_actions() {
        let mut rig = Screen::new();
        rig.rig.button(GO.x + 20.0, GO.y + 12.0, true);
        rig.rig.button(GO.x + 20.0, GO.y + 12.0, false);
        rig.rig.send(WindowEvent::Unfocused);
        rig.frame();
        assert_eq!(
            rig.app.clicks, 0,
            "queued click was cancelled on focus loss"
        );
        rig.rig.button(GO.x + 20.0, GO.y + 12.0, false);
        rig.frame();
        assert_eq!(rig.app.clicks, 0, "no stale press survives focus loss");
        rig.click(GO);
        assert_eq!(rig.app.clicks, 1, "subsequent clicks still work");
    }

    #[test]
    fn a_switch_flips_on_click() {
        let mut rig = Screen::new();
        rig.click(SWITCH);
        assert!(rig.app.on);
        rig.click(SWITCH);
        assert!(!rig.app.on);
    }

    #[test]
    fn tab_walks_the_widgets_and_space_presses() {
        let mut rig = Screen::new();
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
        let mut rig = Screen::new();
        assert!(!rig.rig.send(WindowEvent::TextInput { text: "x".into() }));
        rig.click(NAME);
        assert!(rig.rig.state.text_focused());
        assert!(rig.rig.send(WindowEvent::TextInput {
            text: "hello".into()
        }));
        rig.frame();
        rig.key(KeyCode::Backspace, Modifiers::default());
        assert_eq!(rig.app.name, "hell");
        assert!(
            rig.rig.state.ime_area().is_some(),
            "the input method is told where the caret is"
        );
        rig.key(KeyCode::Escape, Modifiers::default());
        assert!(!rig.rig.state.has_focus());
    }

    #[test]
    fn focusing_from_code_selects_the_field() {
        let mut rig = Screen::new();
        rig.app.name = "old".into();
        rig.rig.state.focus(Id::Name, 0);
        rig.frame();
        assert!(rig.rig.state.text_focused());
        assert!(rig.rig.send(WindowEvent::TextInput { text: "new".into() }));
        rig.frame();
        assert_eq!(rig.app.name, "new", "typing replaced what was selected");
    }

    #[test]
    fn dragging_a_slider_and_its_keys() {
        let mut rig = Screen::new();
        rig.rig.button(LEVEL.x + 2.0, LEVEL.y + 12.0, true);
        rig.frame();
        rig.rig.send(mouse_move(LEVEL.right(), LEVEL.y + 12.0, 0));
        rig.frame();
        assert_eq!(rig.app.level, 1.0);
        rig.rig.button(LEVEL.right(), LEVEL.y + 12.0, false);
        rig.frame();
        rig.key(KeyCode::Left, Modifiers::default());
        assert!(
            (rig.app.level - 0.9).abs() < 1e-4,
            "the step, from the keyboard"
        );
    }

    #[test]
    fn an_idle_ui_asks_for_no_frames() {
        let mut rig = Screen::new();
        rig.frame();
        assert!(!rig.rig.state.wants_frame());
        assert!(
            !rig.rig.send(mouse_move(230.0, 165.0, 0)),
            "moving over nothing needs no frame"
        );
        assert!(
            rig.rig.send(mouse_move(20.0, 20.0, 0)),
            "moving onto a button does"
        );
    }
}

/// A canvas, UI state and theme that UI tests draw frames into and feed input to.
#[cfg(test)]
pub(crate) mod rig {
    use super::{Theme, Ui, UiState, WidgetId};
    use crate::render::{CpuRasterizer, Renderer};
    use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, PointerKind, WindowEvent};

    /// A pointer move to `(x, y)` with `buttons` held.
    pub(crate) fn mouse_move(x: impl Into<f64>, y: impl Into<f64>, buttons: u32) -> WindowEvent {
        WindowEvent::MouseMove {
            x: x.into(),
            y: y.into(),
            buttons: MouseButtons::from_bits(buttons),
            modifiers: Modifiers::default(),
            pointer: PointerKind::Mouse,
            time: Default::default(),
        }
    }

    /// The left button going down or up at `(x, y)`.
    pub(crate) fn left_button(x: impl Into<f64>, y: impl Into<f64>, pressed: bool) -> WindowEvent {
        WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
            modifiers: Modifiers::default(),
            x: x.into(),
            y: y.into(),
            click_count: 1,
            pointer: PointerKind::Mouse,
            time: Default::default(),
        }
    }

    /// A key going down with `modifiers`.
    pub(crate) fn key_down(key: KeyCode, modifiers: Modifiers) -> WindowEvent {
        WindowEvent::KeyInput {
            key,
            pressed: true,
            modifiers,
        }
    }

    pub(crate) struct Rig<Id: WidgetId = u8> {
        pub(crate) raster: CpuRasterizer,
        pub(crate) state: UiState<Id>,
        pub(crate) theme: Theme,
    }

    impl<Id: WidgetId> Rig<Id> {
        /// A `width` by `height` canvas with the dark theme.
        pub(crate) fn new(width: u32, height: u32) -> Self {
            Self {
                raster: CpuRasterizer::new(width, height),
                state: UiState::new(),
                theme: Theme::dark(),
            }
        }

        /// Values land at once instead of easing.
        pub(crate) fn still(mut self) -> Self {
            self.state.set_reduced_motion(true);
            self
        }

        /// Draws one frame with `draw`.
        pub(crate) fn frame<R>(&mut self, draw: impl FnOnce(&mut Ui<'_, Id>) -> R) -> R {
            let mut ctx = self.raster.begin_frame().expect("frame");
            let out = draw(&mut self.state.frame(&mut *ctx, &self.theme));
            drop(ctx);
            self.raster.end_frame().expect("end");
            out
        }

        /// Hands `event` to the state; whether it changed anything.
        pub(crate) fn send(&mut self, event: WindowEvent) -> bool {
            self.state.handle(&event)
        }

        /// Moves the pointer to `(x, y)`, no button held.
        pub(crate) fn at(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
            self.send(mouse_move(x, y, 0));
        }

        /// Moves the pointer to `(x, y)` with the left button held.
        pub(crate) fn drag_to(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
            self.send(mouse_move(x, y, 1));
        }

        /// The left button going down or up at `(x, y)`.
        pub(crate) fn button(&mut self, x: impl Into<f64>, y: impl Into<f64>, pressed: bool) {
            self.send(left_button(x, y, pressed));
        }

        /// A press and release at `(x, y)`.
        pub(crate) fn click(&mut self, x: impl Into<f64> + Copy, y: impl Into<f64> + Copy) {
            self.button(x, y, true);
            self.button(x, y, false);
        }

        /// The pointer at `(x, y)` and the wheel turned by `(dx, dy)` notches.
        pub(crate) fn wheel(
            &mut self,
            x: impl Into<f64>,
            y: impl Into<f64>,
            dx: impl Into<f64>,
            dy: impl Into<f64>,
        ) {
            self.at(x, y);
            self.send(WindowEvent::MouseWheel {
                delta_x: dx.into(),
                delta_y: dy.into(),
                modifiers: Modifiers::default(),
            });
        }

        /// `key` going down, no modifiers.
        pub(crate) fn key(&mut self, key: KeyCode) {
            self.send(key_down(key, Modifiers::default()));
        }

        /// `text` typed.
        pub(crate) fn type_text(&mut self, text: &str) {
            self.send(WindowEvent::TextInput {
                text: text.to_string(),
            });
        }

        /// The drawn pixel at `(x, y)`, as RGBA bytes.
        pub(crate) fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
            let frame = self.raster.frame_output().expect("pixels");
            let at = (y as usize * frame.width as usize + x as usize) * 4;
            let bytes = frame.bytes();
            [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
        }
    }
}

#[cfg(test)]
mod integration;
