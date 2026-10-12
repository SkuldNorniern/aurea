//! Canvas widgets: a small settings panel.
//!
//! Tab through it, type in the fields, drag the slider; the theme switch
//! restyles everything live.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use aurea::elements::{Container, Orientation, Stack};
use aurea::render::{Canvas, RendererBackend, request_canvas_redraw};
use aurea::ui::{
    Align, Button, Checkbox, ProgressBar, Segmented, Slider, TextField, Theme, Toggle, Ui, UiState,
};
use aurea::{AureaResult, Rect, Window, WindowEvent, reduced_motion};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Id {
    Name,
    Password,
    Volume,
    Muted,
    Usage,
    Look,
    Meter,
    Reset,
    Save,
}

struct Settings {
    name: String,
    password: String,
    volume: f32,
    muted: bool,
    usage: bool,
    look: usize,
    saved: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: String::new(),
            password: String::new(),
            volume: 0.6,
            muted: false,
            usage: true,
            look: 1,
            saved: 0,
        }
    }
}

fn panel(ui: &mut Ui<'_, Id>, s: &mut Settings, width: f32) -> AureaResult<()> {
    let p = ui.theme.palette;
    let fonts = ui.fonts();
    let x = 24.0;
    let w = (width - 48.0).max(200.0);
    let row = |y: f32, h: f32| Rect::new(x, y, w, h);
    ui.text(
        "Settings",
        row(18.0, 28.0),
        &fonts.strong,
        p.text,
        Align::Start,
    )?;

    let half = (w - 12.0) / 2.0;
    TextField::new(&mut s.name)
        .placeholder("Display name")
        .max_chars(32)
        .show(ui, Id::Name, Rect::new(x, 56.0, half, 32.0))?;
    TextField::new(&mut s.password)
        .placeholder("Password")
        .password(true)
        .show(
            ui,
            Id::Password,
            Rect::new(x + half + 12.0, 56.0, half, 32.0),
        )?;

    ui.text(
        "Volume",
        row(102.0, 20.0),
        &fonts.small,
        p.dim,
        Align::Start,
    )?;
    let shown = format!("{:.0}%", s.volume * 100.0);
    ui.text(&shown, row(102.0, 20.0), &fonts.small, p.dim, Align::End)?;
    Slider::new(&mut s.volume, 0.0..=1.0)
        .step(0.01)
        .enabled(!s.muted)
        .show(ui, Id::Volume, row(122.0, 24.0))?;
    Toggle::new(&mut s.muted)
        .label("Mute")
        .show(ui, Id::Muted, row(158.0, 30.0))?;
    Checkbox::new(&mut s.usage)
        .label("Share anonymous usage data")
        .show(ui, Id::Usage, row(196.0, 30.0))?;
    Segmented::new(&["Light", "Dark"], &mut s.look).show(
        ui,
        Id::Look,
        Rect::new(x, 238.0, 200.0, 32.0),
    )?;

    ProgressBar::new(Some(s.volume)).show(ui, Id::Meter, row(286.0, 4.0))?;

    let save = Rect::new(x + w - 96.0, 312.0, 96.0, 32.0);
    let reset = Rect::new(save.x - 96.0 - 10.0, 312.0, 96.0, 32.0);
    let r = Button::new("Reset").quiet().show(ui, Id::Reset, reset)?;
    ui.tooltip(&r, "Put every setting back to its default");
    if r.clicked {
        let saved = s.saved;
        *s = Settings {
            saved,
            ..Settings::default()
        };
    }
    if Button::new("Save")
        .primary()
        .enabled(!s.name.is_empty())
        .show(ui, Id::Save, save)?
        .clicked
    {
        s.saved += 1;
    }
    if s.saved > 0 {
        let note = format!(
            "Saved {} time{}",
            s.saved,
            if s.saved == 1 { "" } else { "s" }
        );
        ui.text(
            &note,
            Rect::new(x, 312.0, w / 2.0, 32.0),
            &fonts.small,
            p.success,
            Align::Start,
        )?;
    }
    Ok(())
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Widgets", 480, 370)?;
    let canvas = Canvas::new(480, 370, RendererBackend::Cpu)?;
    let id = canvas.id();

    let state = Rc::new(RefCell::new(UiState::<Id>::new()));
    state.borrow_mut().set_reduced_motion(reduced_motion());
    let settings = Rc::new(RefCell::new(Settings::default()));
    let themes = [Theme::light(), Theme::dark()];
    let window_ref: Rc<RefCell<Weak<Window>>> = Rc::new(RefCell::new(Weak::new()));

    let drawn = Rc::clone(&state);
    let seen = Rc::clone(&window_ref);
    canvas.set_draw_callback(move |ctx| {
        let mut state = drawn.borrow_mut();
        let mut s = settings.borrow_mut();
        let theme = &themes[s.look.min(1)];
        ctx.clear(theme.palette.background)?;
        #[allow(clippy::cast_precision_loss)]
        let width = ctx.width() as f32;
        {
            let mut ui = state.frame(ctx, theme);
            panel(&mut ui, &mut s, width)?;
        }
        if let Some(window) = seen.borrow().upgrade() {
            state.sync_window(&window);
        }
        if state.wants_frame() {
            request_canvas_redraw(id);
        }
        Ok(())
    })?;

    let input = Rc::clone(&state);
    window.on_event(move |event| {
        if input.borrow_mut().handle(&event) || matches!(event, WindowEvent::Resized { .. }) {
            request_canvas_redraw(id);
        }
    });

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    let window = Rc::new(window);
    *window_ref.borrow_mut() = Rc::downgrade(&window);
    window.show();
    window.run()
}
