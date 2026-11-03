//! One start-up path on every platform, with a frame that can say it wants another.

use aurea::render::Rect;
use aurea::ui::{Align, Button, FrameAsk, Theme};
use aurea::{AureaResult, app};

fn main() -> AureaResult<()> {
    let mut presses = 0_u32;
    app::run(Theme::dark(), move |ui, width, height| {
        let fonts = ui.fonts();
        let color = ui.theme.palette.text;
        let line = Rect::new(16.0, 16.0, width - 32.0, 24.0);
        ui.text_fmt(
            format_args!("{width:.0} by {height:.0}, pressed {presses} times"),
            line,
            &fonts.text,
            color,
            Align::Start,
        )?;
        let button = Rect::new(16.0, 52.0, 160.0, ui.theme.control_height);
        if Button::new("Press").show(ui, 1, button)?.clicked {
            presses += 1;
        }
        Ok(FrameAsk::default())
    })
}
