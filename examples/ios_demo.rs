//! A small Aurea app for the iOS sample in `mobile/ios/`: counter, slider, toggles, and a pad that
//! draws with Apple Pencil pressure. The top right line shows the last key, pinch or lifecycle
//! event it heard. The sample's `main` calls [`aurea_main`].

#[cfg(target_os = "ios")]
mod app {
    use aurea::ios::{on_event, run as run_app, set_theme};
    use aurea::ui::{Align, Button, Checkbox, ProgressBar, Segmented, Slider, Theme, Toggle, Ui};
    use aurea::{AureaResult, PenPhase, PenSample, Point, Rect, WindowEvent};
    use std::cell::RefCell;
    use std::rc::Rc;

    const ADD: u64 = 1;
    const RESET: u64 = 2;
    const LEVEL: u64 = 3;
    const MUTED: u64 = 4;
    const SHARE: u64 = 5;
    const LOOK: u64 = 6;
    const METER: u64 = 7;
    const CLEAR: u64 = 8;

    /// Where the pad starts, below the controls.
    const PAD_TOP: f32 = 448.0;

    /// Pencil strokes: each point with the width its pressure gave it.
    #[derive(Default)]
    struct Sketch {
        strokes: Vec<Vec<(Point, f32)>>,
        drawing: bool,
        last: Option<PenSample>,
    }

    impl Sketch {
        fn pen(&mut self, sample: PenSample) {
            #[allow(clippy::cast_possible_truncation)]
            let at = Point::new(sample.x as f32, sample.y as f32);
            let width = 1.0 + sample.pressure * 9.0;
            match sample.phase {
                PenPhase::Down => {
                    self.drawing = at.y > PAD_TOP;
                    if self.drawing {
                        self.strokes.push(vec![(at, width)]);
                    }
                }
                PenPhase::Move | PenPhase::Up if self.drawing => {
                    if let Some(stroke) = self.strokes.last_mut() {
                        stroke.push((at, width));
                    }
                    self.drawing = sample.phase == PenPhase::Move;
                }
                _ => {}
            }
            self.last = Some(sample);
        }
    }

    /// The last event worth showing that is not a touch or the pen.
    fn heard(event: &WindowEvent) -> Option<String> {
        Some(match event {
            WindowEvent::KeyInput {
                key, pressed: true, ..
            } => format!("key {key:?}"),
            WindowEvent::TextInput { text } => format!("typed {text:?}"),
            WindowEvent::Magnify { factor, .. } => format!("pinch x{factor:.2}"),
            WindowEvent::MouseWheel {
                delta_x, delta_y, ..
            } => format!("scroll {delta_x:.0}, {delta_y:.0}"),
            WindowEvent::Focused => "active".to_owned(),
            WindowEvent::Unfocused => "inactive".to_owned(),
            WindowEvent::Minimized => "background".to_owned(),
            WindowEvent::Restored => "foreground".to_owned(),
            WindowEvent::MemoryWarning => "memory warning".to_owned(),
            _ => return None,
        })
    }

    #[derive(Default)]
    struct State {
        count: u32,
        level: f32,
        muted: bool,
        share: bool,
        look: usize,
    }

    fn pad(ui: &mut Ui<'_, u64>, sketch: &mut Sketch, width: f32, height: f32) -> AureaResult<()> {
        let p = ui.theme.palette;
        let fonts = ui.fonts();
        let area = Rect::new(
            20.0,
            PAD_TOP,
            (width - 40.0).max(160.0),
            (height - PAD_TOP - 20.0).max(80.0),
        );
        ui.border(area, 12.0, 1.0, p.dim)?;
        let reading = sketch
            .last
            .map_or("Draw here with Apple Pencil".to_owned(), |s| {
                format!(
                    "pressure {:.2}  tilt {:.0}°, {:.0}°",
                    s.pressure, s.tilt_x, s.tilt_y
                )
            });
        let caption = Rect::new(area.x + 12.0, area.y + 8.0, area.width - 100.0, 24.0);
        ui.text(&reading, caption, &fonts.small, p.dim, Align::Start)?;
        let clear = Rect::new(area.x + area.width - 84.0, area.y + 4.0, 76.0, 32.0);
        if Button::new("Clear").quiet().show(ui, CLEAR, clear)?.clicked {
            sketch.strokes.clear();
        }
        for stroke in &sketch.strokes {
            for pair in stroke.windows(2) {
                let ((a, wa), (b, wb)) = (pair[0], pair[1]);
                let width = (wa + wb) / 2.0;
                ui.line(a, b, width, p.text)?;
                ui.circle(b, width / 2.0, p.text)?;
            }
        }
        Ok(())
    }

    fn panel(ui: &mut Ui<'_, u64>, s: &mut State, last: &str, width: f32) -> AureaResult<()> {
        let p = ui.theme.palette;
        let fonts = ui.fonts();
        let (x, w) = (20.0, (width - 40.0).max(160.0));
        let row = |y: f32, h: f32| Rect::new(x, y, w, h);

        ui.text(
            "Aurea on iOS",
            row(12.0, 32.0),
            &fonts.strong,
            p.text,
            Align::Start,
        )?;
        ui.text(last, row(12.0, 32.0), &fonts.small, p.dim, Align::End)?;
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
        if Button::new("Reset")
            .quiet()
            .show(ui, RESET, Rect::new(x + half + 12.0, 96.0, half, 48.0))?
            .clicked
        {
            s.count = 0;
        }

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
        Ok(())
    }

    pub fn run() -> ! {
        let mut state = State {
            level: 0.6,
            look: 1,
            ..State::default()
        };
        let sketch = Rc::new(RefCell::new(Sketch::default()));
        let last = Rc::new(RefCell::new(String::new()));
        let (pen, shown) = (Rc::clone(&sketch), Rc::clone(&last));
        on_event(move |event| {
            if let WindowEvent::Pen(sample) = event {
                pen.borrow_mut().pen(*sample);
            } else if let Some(text) = heard(event) {
                *shown.borrow_mut() = text;
            }
        });
        run_app(Theme::dark(), move |ui, width, height| {
            panel(ui, &mut state, &last.borrow(), width)?;
            pad(ui, &mut sketch.borrow_mut(), width, height)
        })
    }
}

#[cfg(target_os = "ios")]
use std::os::raw::c_int;

/// The app's entry point, called by the Xcode sample's `main`; UIKit runs from here on.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn aurea_main() -> c_int {
    app::run()
}
