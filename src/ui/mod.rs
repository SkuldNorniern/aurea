//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every
//! platform.
//!
//! They are immediate: each frame the draw callback says what is on screen
//! and where, and each widget answers what the input did to it. There is no
//! tree to keep in sync with the app's state, which stays the app's own.
//!
//! # Layers
//!
//! Each layer is made of the one under it, and every layer is public, so an
//! app builds at whichever height fits.
//!
//! 1. [`Ui`] itself: [`Ui::interact`] puts something on the hit list and
//!    reports the input on it, [`Ui::eased`] and [`Ui::animate`] keep values
//!    moving, and shape and text calls draw.
//! 2. [`paint`]: pieces that only draw. [`Surface`], [`Label`], [`Mark`],
//!    [`Separator`], [`Badge`], [`Spinner`], and a plot's [`GridLines`],
//!    [`AxisMarks`], [`Trace`] and [`Marker`].
//! 3. [`component`]: pieces that follow input. [`StateColors`],
//!    [`FocusRing`], [`Track`], [`Knob`], [`EditableText`],
//!    [`PanZoom`] for an editor's view, and [`PlotView`] for a plot's.
//! 4. [`widget`]: each a few components and [`Ui::interact`] calls, with a
//!    style made from the theme that can be changed field by field.
//!    - Actions: [`Button`], [`Link`].
//!    - Choices: [`Toggle`], [`Checkbox`], [`RadioGroup`], [`Chip`],
//!      [`Segmented`], [`Dropdown`], [`Swatches`], [`ColorPicker`].
//!    - Values: [`Slider`], [`Stepper`], [`TextField`], [`ProgressBar`].
//!    - Data: [`GraphView`] and [`ScopeView`], over the app's own
//!      [`graph::Graph`] and [`graph::Scope`].
//!    - Structure: [`TabBar`], [`Pager`], [`Disclosure`], [`ScrollArea`],
//!      [`ListView`], [`EmptyState`], and [`DockView`] for panels in
//!      tabbed groups the user can split and drag.
//!    - Over the rest: tooltips ([`Ui::tooltip`]) and context menus
//!      ([`Ui::open_menu`]).
//!
//! An app's own widget is the same shape as the built-in ones:
//!
//! ```rust,no_run
//! use aurea::ui::{Component, FocusRing, Label, Response, Sense, StateColors, Surface, Ui, Widget, WidgetId};
//! use aurea::{AureaResult, Rect};
//!
//! /// A card that lights up and can be picked.
//! struct Tile<'t> { title: &'t str, picked: bool }
//!
//! impl<Id: WidgetId> Widget<Id> for Tile<'_> {
//!     fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
//!         let input = ui.interact(id, 0, rect, Sense::Click, true);
//!         let state = ui.eased(id, 0, input);
//!         let p = ui.theme.palette;
//!         let face = StateColors::new(p.surface, p.hover, p.pressed).at(state);
//!         let edge = if self.picked { p.accent } else { p.border };
//!         FocusRing::new(ui.theme, 10.0, state.focus).paint(ui, rect)?;
//!         Surface::new(face).radius(10.0).border(1.5, edge).paint(ui, rect)?;
//!         Label::new(self.title, p.text).strong().paint(ui, rect.inset(12.0))?;
//!         Ok(Response::new(rect, input))
//!     }
//! }
//! ```
//!
//! # Wiring it to a canvas
//!
//! ```rust,no_run
//! use std::{cell::RefCell, rc::Rc};
//! use aurea::render::{Canvas, RendererBackend, request_canvas_redraw};
//! use aurea::ui::{Button, Slider, TextField, Theme, Toggle, UiState};
//! use aurea::{Rect, Window};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! enum Id { Name, Volume, Muted, Save }
//!
//! #[derive(Default)]
//! struct App { name: String, volume: f32, muted: bool }
//!
//! # fn main() -> aurea::AureaResult<()> {
//! let window = Window::new("Settings", 360, 240)?;
//! let canvas = Canvas::new(360, 240, RendererBackend::Cpu)?;
//! let ui = Rc::new(RefCell::new(UiState::<Id>::new()));
//! ui.borrow_mut().set_reduced_motion(aurea::reduced_motion());
//! let app = Rc::new(RefCell::new(App::default()));
//! let theme = Theme::dark();
//! let id = canvas.id();
//!
//! let drawn = Rc::clone(&ui);
//! canvas.set_draw_callback(move |ctx| {
//!     let mut state = drawn.borrow_mut();
//!     let mut app = app.borrow_mut();
//!     {
//!         let mut ui = state.frame(ctx, &theme);
//!         let row = Rect::new(20.0, 20.0, 320.0, 30.0);
//!         TextField::new(&mut app.name).placeholder("Name").show(&mut ui, Id::Name, row)?;
//!         let row = Rect::new(20.0, 66.0, 320.0, 24.0);
//!         Slider::new(&mut app.volume, 0.0..=1.0).show(&mut ui, Id::Volume, row)?;
//!         let row = Rect::new(20.0, 106.0, 320.0, 28.0);
//!         Toggle::new(&mut app.muted).label("Muted").show(&mut ui, Id::Muted, row)?;
//!         let at = Rect::new(244.0, 190.0, 96.0, 30.0);
//!         if Button::new("Save").primary().show(&mut ui, Id::Save, at)?.clicked {
//!             // save app
//!         }
//!     }
//!     // Easing, a blinking caret or a click want the next frame too.
//!     if state.wants_frame() {
//!         request_canvas_redraw(id);
//!     }
//!     Ok(())
//! })?;
//!
//! window.on_event(move |event| {
//!     if ui.borrow_mut().handle(&event) {
//!         request_canvas_redraw(id);
//!     }
//! });
//! # Ok(())
//! # }
//! ```
//!
//! Keyboard: Tab and Shift+Tab move between widgets, Enter and Space press
//! buttons and switches, arrows move sliders, and text fields take the
//! usual editing keys, clipboard shortcuts and undo, with the platform's
//! own modifiers. [`UiState::sync_window`] keeps the pointer shape and an
//! input method's window following the widgets.

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
pub use frame::{Align, Eased, PressAt, Response, Ui};
pub use paint::{
    AxisMarks, AxisSide, AxisStyle, Badge, BadgeTone, Border, Component, GridLines, GridStyle,
    IconMark, IconSource, Label, Mark, MarkShape, Marker, Overflow, Separator, Shadow, Spinner,
    Stroke, Surface, TextSize, Trace,
};
pub use state::{Channel, Fonts, Interaction, Sense, UiState, WidgetId};
pub use theme::{Motion, Palette, Theme};
pub use widget::Widget;
pub use widget::button::{Button, ButtonStyle, Tone};
pub use widget::chip::{Chip, ChipStyle};
pub use widget::color::{ColorPicker, PickerStyle};
pub use widget::disclosure::{Disclosure, DisclosureStyle};
pub use widget::dock::{
    Dock, DockAxis, DockBar, DockGroup, DockLayout, DockMetrics, DockPlan, DockSide, DockStyle,
    DockTree, DockView,
};
pub use widget::dropdown::{Dropdown, DropdownStyle};
pub use widget::empty::EmptyState;
pub use widget::graph;
pub use widget::graph::{GraphStyle, GraphView, ScopeStyle, ScopeView};
pub use widget::link::{Link, LinkStyle};
pub use widget::pager::{Pager, PagerStyle};
pub use widget::popup::MenuItem;
pub use widget::progress::{ProgressBar, ProgressStyle};
pub use widget::radio::{RadioGroup, RadioStyle};
pub use widget::scroll::{
    ListRow, ListView, ScrollArea, ScrollStyle, scroll_into_view, visible_rows,
};
pub use widget::segmented::{Segmented, SegmentedStyle};
pub use widget::slider::{Slider, SliderStyle};
pub use widget::stepper::{Stepper, StepperStyle};
pub use widget::swatch::{SwatchStyle, Swatches};
pub use widget::tabs::{TabBar, TabStyle};
pub use widget::text_field::{FieldStyle, TextField};
pub use widget::toggle::{Checkbox, CheckboxStyle, Toggle, ToggleStyle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_on_either_side_of_tab_goes_to_its_own_field() {
        for reverse in [false, true] {
            let mut state = UiState::<u8>::new();
            let theme = Theme::dark();
            let mut raster = CpuRasterizer::new(160, 100);
            let (mut first, mut second) = (String::new(), String::new());
            let frame = |state: &mut UiState<u8>,
                         raster: &mut CpuRasterizer,
                         first: &mut String,
                         second: &mut String,
                         reverse: bool| {
                let mut ctx = raster.begin_frame().expect("frame");
                let mut ui = state.frame(&mut *ctx, &theme);
                let a = Rect::new(0.0, 0.0, 120.0, 32.0);
                let b = Rect::new(0.0, 40.0, 120.0, 32.0);
                if reverse {
                    TextField::new(second).show(&mut ui, 2, b).expect("second");
                    TextField::new(first).show(&mut ui, 1, a).expect("first");
                } else {
                    TextField::new(first).show(&mut ui, 1, a).expect("first");
                    TextField::new(second).show(&mut ui, 2, b).expect("second");
                }
            };
            let tab = WindowEvent::KeyInput {
                key: KeyCode::Tab,
                pressed: true,
                modifiers: Modifiers::default(),
            };
            frame(&mut state, &mut raster, &mut first, &mut second, false);
            state.handle(&tab);
            frame(&mut state, &mut raster, &mut first, &mut second, false);
            state.handle(&WindowEvent::TextInput { text: "a".into() });
            state.handle(&tab);
            state.handle(&WindowEvent::TextInput { text: "b".into() });
            frame(&mut state, &mut raster, &mut first, &mut second, reverse);
            assert_eq!(first, "a");
            assert_eq!(second, "b");
            state.handle(&WindowEvent::TextInput { text: "c".into() });
            frame(&mut state, &mut raster, &mut first, &mut second, reverse);
            assert_eq!(second, "bc");
        }
    }
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
    fn focusing_from_code_selects_the_field() {
        let mut rig = Rig::new();
        rig.app.name = "old".into();
        rig.ui.focus(Id::Name, 0);
        rig.frame();
        assert!(rig.ui.text_focused());
        assert!(rig.send(WindowEvent::TextInput { text: "new".into() }));
        rig.frame();
        assert_eq!(rig.app.name, "new", "typing replaced what was selected");
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
