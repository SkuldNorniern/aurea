//! The built-in icons at three sizes, and an SVG drawn from text.

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, Color, DrawingContext, Icon, Rect, RendererBackend, VectorImage};
use aurea::{AureaResult, Container, Window};

const BADGE: &str = r##"<svg viewBox="0 0 32 32">
  <circle cx="16" cy="16" r="14" fill="#3b82f6"/>
  <path d="M10 16.5 l4 4 l8 -9" fill="none" stroke="#fff" stroke-width="3"/>
</svg>"##;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Icons", 800, 300)?;
    let canvas = Canvas::new(800, 300, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(250, 250, 252));
    let badge = VectorImage::parse(BADGE).expect("badge svg");
    canvas.set_draw_callback(move |ctx: &mut dyn DrawingContext| {
        let ink = Color::rgb(40, 44, 52);
        for (row, size) in [16.0_f32, 24.0, 48.0].into_iter().enumerate() {
            let y = 16.0 + row as f32 * 70.0;
            for (i, icon) in Icon::ALL.iter().enumerate().take(14) {
                let x = 16.0 + i as f32 * (size + 4.0).max(52.0);
                icon.draw(ctx, Rect::new(x, y, size, size), ink)?;
            }
        }
        for (i, icon) in Icon::ALL.iter().enumerate().skip(14) {
            let x = 16.0 + (i - 14) as f32 * 52.0;
            icon.draw(ctx, Rect::new(x, 230.0, 24.0, 24.0), ink)?;
        }
        badge.draw(ctx, Rect::new(748.0, 222.0, 40.0, 40.0), ink)
    })?;
    let mut stack = Stack::new(Orientation::Vertical)?;
    stack.set_padding(0)?;
    stack.add(canvas)?;
    window.set_content(stack)?;
    window.run()
}
