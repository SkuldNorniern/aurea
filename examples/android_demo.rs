//! A small Aurea app for the Android sample in `mobile/android/`: counter, slider, toggles.

#[cfg(target_os = "android")]
mod app {
    use aurea::android::{set_app, set_theme};
    use aurea::ui::{Align, Button, Checkbox, ProgressBar, Segmented, Slider, Theme, Toggle, Ui};
    use aurea::{AureaResult, Rect};
    use jni::JNIEnv;
    use jni::objects::JObject;

    const ADD: u64 = 1;
    const RESET: u64 = 2;
    const LEVEL: u64 = 3;
    const MUTED: u64 = 4;
    const SHARE: u64 = 5;
    const LOOK: u64 = 6;
    const METER: u64 = 7;

    #[derive(Default)]
    struct State {
        count: u32,
        level: f32,
        muted: bool,
        share: bool,
        look: usize,
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
        Ok(())
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeCreateApp(
        _env: JNIEnv,
        _thiz: JObject,
    ) {
        let mut state = State {
            level: 0.6,
            look: 1,
            ..State::default()
        };
        set_app(Theme::dark(), move |ui, width, _height| {
            panel(ui, &mut state, width)
        });
    }
}
