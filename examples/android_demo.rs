//! A small Aurea app for the Android sample in `mobile/android/`: counter, slider, toggles, and a
//! field the on-screen keyboard types into. Below them: the screen's insets and corners, the system
//! language, a line of hangul, a menu to scroll by finger, and the last gesture, finger tap or back
//! press it heard. Long press Reset for its tooltip. Back resets the count, then leaves.

#[cfg(target_os = "android")]
mod app {
    use std::cell::RefCell;
    use std::ffi::c_int;
    use std::rc::Rc;

    use aurea::android::{on_back, on_event, request_frame, run, set_theme, show_keyboard};
    use aurea::app::take_opened;
    use aurea::ui::{
        Align, Button, Checkbox, MenuItem, ProgressBar, Segmented, Slider, TextField, Theme,
        Toggle, Ui,
    };
    use aurea::{AureaResult, GesturePhase, Point, Rect, WindowEvent, keyboard_connected, locale};

    const ADD: u64 = 1;
    const RESET: u64 = 2;
    const LEVEL: u64 = 3;
    const MUTED: u64 = 4;
    const SHARE: u64 = 5;
    const LOOK: u64 = 6;
    const METER: u64 = 7;
    const NOTE: u64 = 8;
    const LONG_MENU: u64 = 9;

    #[derive(Default)]
    struct State {
        count: u32,
        level: f32,
        muted: bool,
        share: bool,
        look: usize,
        note: String,
        picked: Option<usize>,
        opened: Vec<String>,
    }

    /// The last event worth showing.
    fn heard(event: &WindowEvent) -> Option<String> {
        Some(match *event {
            WindowEvent::Gesture {
                phase,
                dx,
                dy,
                scale,
                turn,
                ..
            } => {
                let name = match phase {
                    GesturePhase::Began => "began",
                    GesturePhase::Moved => "moved",
                    GesturePhase::Ended => "ended",
                    GesturePhase::Cancelled => "cancelled",
                };
                format!("gesture {name} {dx:.0},{dy:.0} x{scale:.2} {turn:.2} rad")
            }
            WindowEvent::MouseWheel {
                delta_x, delta_y, ..
            } => format!("wheel {delta_x:.0},{delta_y:.0}"),
            WindowEvent::FingerTap { fingers, .. } => format!("{fingers} finger tap"),
            WindowEvent::LongPress { .. } => "long press".to_owned(),
            WindowEvent::OpenRequested => "open requested".to_owned(),
            _ => return None,
        })
    }

    fn panel(ui: &mut Ui<'_, u64>, s: &mut State, last: &str, width: f32) -> AureaResult<()> {
        let p = ui.theme.palette;
        let fonts = ui.fonts();
        let (x, w) = (20.0, (width - 40.0).max(160.0));
        let row = |y: f32, h: f32| Rect::new(x, y, w, h);

        ui.text(
            "Aurea on Android",
            row(12.0, 32.0),
            &fonts.strong,
            p.text,
            Align::Start,
        )?;
        let shown = format!("Tapped {} times", s.count);
        ui.text(&shown, row(60.0, 24.0), &fonts.text, p.dim, Align::Start)?;
        let half = (w - 12.0) / 2.0;
        if Button::new("Tap")
            .primary()
            .show(ui, ADD, Rect::new(x, 96.0, half, 48.0))?
            .clicked
        {
            s.count += 1;
        }
        let reset = Button::new("Reset").quiet().show(
            ui,
            RESET,
            Rect::new(x + half + 12.0, 96.0, half, 48.0),
        )?;
        if reset.clicked {
            s.count = 0;
        }
        ui.tooltip(&reset, "Sets the count back to zero");

        let level = format!("Level {:.0}%", s.level * 100.0);
        ui.text(&level, row(168.0, 24.0), &fonts.small, p.dim, Align::Start)?;
        Slider::new(&mut s.level, 0.0..=1.0)
            .enabled(!s.muted)
            .show(ui, LEVEL, row(196.0, 40.0))?;
        ProgressBar::new(Some(s.level)).show(ui, METER, row(248.0, 6.0))?;
        Toggle::new(&mut s.muted)
            .label("Mute")
            .show(ui, MUTED, row(272.0, 44.0))?;
        Checkbox::new(&mut s.share)
            .label("Share usage data")
            .show(ui, SHARE, row(324.0, 44.0))?;
        let look = s.look;
        Segmented::new(&["Light", "Dark"], &mut s.look).show(ui, LOOK, row(384.0, 44.0))?;
        if s.look != look {
            set_theme(if s.look == 0 {
                Theme::light()
            } else {
                Theme::dark()
            });
        }
        let note = TextField::new(&mut s.note).placeholder("Type here").show(
            ui,
            NOTE,
            row(444.0, 44.0),
        )?;
        if note.editing_started {
            show_keyboard(true);
        }
        if note.focus_lost {
            show_keyboard(false);
        }

        let open = Button::new("Long menu").show(ui, LONG_MENU, row(500.0, 44.0))?;
        if open.clicked {
            let labels: Vec<String> = (1..=40).map(|n| format!("Row {n}")).collect();
            ui.open_menu(
                LONG_MENU,
                Point::new(x, 500.0),
                labels.iter().map(|label| MenuItem::new(label)),
            );
        }
        if let Some(row) = ui.menu_picked(LONG_MENU) {
            s.picked = Some(row);
        }
        let screen = ui.screen();
        let lines = [
            format!("safe area {:?}", screen.safe_area),
            format!("gesture zones {:?}", screen.gestures),
            format!("corners {:?}", screen.corners),
            format!(
                "locale {:?}, picked {:?}, keyboard {}",
                locale(),
                s.picked,
                keyboard_connected()
            ),
            "한글 표시 시험".to_owned(),
            format!("heard {last}"),
            format!("opened {}", s.opened.join(", ")),
        ];
        for (line, y) in lines
            .iter()
            .zip((0u8..).map(|n| 552.0 + f32::from(n) * 24.0))
        {
            ui.text(line, row(y, 22.0), &fonts.small, p.dim, Align::Start)?;
        }
        Ok(())
    }

    /// The app's entry; `AureaActivity` calls it once.
    #[unsafe(no_mangle)]
    pub extern "C" fn aurea_main() -> c_int {
        let mut state = State {
            level: 0.6,
            look: 1,
            ..State::default()
        };
        let last = Rc::new(RefCell::new(String::new()));
        let sink = Rc::clone(&last);
        on_event(move |event| {
            if let Some(line) = heard(event) {
                *sink.borrow_mut() = line;
                request_frame();
            }
        });
        let count = Rc::new(RefCell::new(0u32));
        let back = Rc::clone(&count);
        on_back(move || {
            // nothing to undo: leave it to the system.
            let took = *back.borrow() > 0;
            *back.borrow_mut() = u32::MAX;
            took
        });
        run(Theme::dark(), move |ui, width, _height| {
            if *count.borrow() == u32::MAX {
                state.count = 0;
            }
            *count.borrow_mut() = state.count;
            state.opened.extend(
                take_opened()
                    .iter()
                    .filter_map(|path| path.file_name()?.to_str().map(str::to_owned)),
            );
            panel(ui, &mut state, &last.borrow(), width)
        });
        0
    }
}
