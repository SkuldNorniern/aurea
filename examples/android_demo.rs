//! A small Aurea app for the Android sample in `mobile/android/`: counter, slider, toggles, and a
//! field the on-screen keyboard types into.

#[cfg(target_os = "android")]
mod app {
    use std::ffi::c_int;

    use aurea::android::{run, set_theme, show_keyboard};
    use aurea::ui::{
        Align, Button, Checkbox, ProgressBar, Segmented, Slider, TextField, Theme, Toggle, Ui,
    };
    use aurea::{AureaResult, Rect};

    const ADD: u64 = 1;
    const RESET: u64 = 2;
    const LEVEL: u64 = 3;
    const MUTED: u64 = 4;
    const SHARE: u64 = 5;
    const LOOK: u64 = 6;
    const METER: u64 = 7;
    const NOTE: u64 = 8;

    #[derive(Default)]
    struct State {
        count: u32,
        level: f32,
        muted: bool,
        share: bool,
        look: usize,
        note: String,
    }

    fn panel(ui: &mut Ui<'_, u64>, s: &mut State, width: f32) -> AureaResult<()> {
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
        run(Theme::dark(), move |ui, width, _height| {
            panel(ui, &mut state, width)
        });
        0
    }
}
