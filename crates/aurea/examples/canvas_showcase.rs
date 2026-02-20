//! Representative canvas example: one scene that shows the full CPU canvas stack.
//!
//! Covers anti-aliased primitives (rect, circle, path), gradients, blend modes,
//! and image drawing so you can see what the rasterizer supports in a single run.

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{
    BlendMode, Canvas, Color, GradientStop, Image, LinearGradient, Paint, PaintStyle, Path,
    PathCommand, Point, RadialGradient, Rect, RendererBackend,
};
use aurea::{AureaResult, Container, Window};

const W: u32 = 800;
const H: u32 = 560;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Canvas Showcase", W as i32, H as i32)?;
    let mut canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(250, 248, 245));

    canvas.draw(|ctx| draw_showcase(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_showcase(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    draw_primitives(ctx)?;
    draw_gradients(ctx)?;
    draw_blend(ctx)?;
    draw_image(ctx)?;
    draw_border(ctx)?;
    Ok(())
}

fn draw_primitives(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let rect_fill = Paint::new()
        .color(Color::rgb(120, 160, 200))
        .style(PaintStyle::Fill);
    ctx.draw_rect(Rect::new(24.0, 24.0, 140.0, 100.0), &rect_fill)?;

    let rect_stroke = Paint::new()
        .color(Color::rgb(180, 100, 100))
        .style(PaintStyle::Stroke)
        .stroke_width(2.0);
    ctx.draw_rect(Rect::new(180.0, 24.0, 140.0, 100.0), &rect_stroke)?;

    let circle_fill = Paint::new()
        .color(Color::rgb(100, 180, 120))
        .style(PaintStyle::Fill);
    ctx.draw_circle(Point::new(380.0, 74.0), 50.0, &circle_fill)?;

    let path = path_triangle(Point::new(520.0, 44.0), 60.0);
    let path_fill = Paint::new()
        .color(Color::rgb(200, 160, 100))
        .style(PaintStyle::Fill);
    ctx.draw_path(&path, &path_fill)?;

    Ok(())
}

fn path_triangle(origin: Point, size: f32) -> Path {
    let mut p = Path::new();
    p.commands.push(PathCommand::MoveTo(Point::new(origin.x + size * 0.5, origin.y)));
    p.commands.push(PathCommand::LineTo(Point::new(origin.x + size, origin.y + size)));
    p.commands.push(PathCommand::LineTo(Point::new(origin.x, origin.y + size)));
    p.commands.push(PathCommand::Close);
    p
}

fn draw_gradients(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let linear = LinearGradient {
        start: Point::new(24.0, 160.0),
        end: Point::new(200.0, 260.0),
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(220, 120, 120),
            },
            GradientStop {
                offset: 0.5,
                color: Color::rgb(255, 255, 200),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(120, 120, 220),
            },
        ],
    };
    ctx.fill_linear_gradient(&linear, Rect::new(24.0, 160.0, 200.0, 120.0))?;

    let radial = RadialGradient {
        center: Point::new(320.0, 220.0),
        radius: 80.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(255, 240, 200),
            },
            GradientStop {
                offset: 0.6,
                color: Color::rgb(220, 180, 100),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(140, 100, 60),
            },
        ],
    };
    ctx.fill_radial_gradient(&radial, Rect::new(260.0, 160.0, 120.0, 120.0))?;

    Ok(())
}

fn draw_blend(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let base = Paint::new().color(Color::rgb(200, 190, 170));
    ctx.draw_rect(Rect::new(24.0, 300.0, 220.0, 120.0), &base)?;

    ctx.set_blend_mode(BlendMode::Multiply)?;
    ctx.draw_rect(
        Rect::new(44.0, 320.0, 80.0, 50.0),
        &Paint::new().color(Color::rgba(200, 100, 100, 220)),
    )?;
    ctx.draw_rect(
        Rect::new(120.0, 350.0, 80.0, 50.0),
        &Paint::new().color(Color::rgba(100, 180, 100, 220)),
    )?;

    ctx.set_blend_mode(BlendMode::Screen)?;
    ctx.draw_circle(
        Point::new(180.0, 360.0),
        45.0,
        &Paint::new().color(Color::rgba(100, 100, 255, 180)),
    )?;

    ctx.set_blend_mode(BlendMode::Normal)?;
    Ok(())
}

fn draw_image(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let image = make_showcase_image();
    ctx.draw_image_rect(&image, Rect::new(280.0, 300.0, 160.0, 120.0))?;

    let border = Paint::new()
        .color(Color::rgb(180, 175, 170))
        .style(PaintStyle::Stroke)
        .stroke_width(1.0);
    ctx.draw_rect(Rect::new(280.0, 300.0, 160.0, 120.0), &border)?;

    Ok(())
}

fn make_showcase_image() -> Image {
    const W: u32 = 64;
    const H: u32 = 48;
    let mut data = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) as usize * 4;
            data[i] = (x * 4).min(255) as u8;
            data[i + 1] = (y * 5).min(255) as u8;
            data[i + 2] = 200;
            data[i + 3] = 255;
        }
    }
    Image::new(W, H, data)
}

fn draw_border(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let border = Paint::new()
        .color(Color::rgb(80, 75, 70))
        .style(PaintStyle::Stroke)
        .stroke_width(2.0);
    ctx.draw_rect(
        Rect::new(1.0, 1.0, W as f32 - 2.0, H as f32 - 2.0),
        &border,
    )?;
    Ok(())
}
