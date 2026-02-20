//! Canvas gradient demo: fill_linear_gradient, fill_radial_gradient.
//!
//! Draws linear and radial gradients to verify the gradient fill path.

use aurea::elements::{Box, BoxOrientation};
use aurea::render::{
    Canvas, Color, GradientStop, LinearGradient, Point, RadialGradient, Rect, RendererBackend,
};
use aurea::{AureaResult, Container, Window};

const CANVAS_WIDTH: u32 = 640;
const CANVAS_HEIGHT: u32 = 480;

fn main() -> AureaResult<()> {
    let mut window =
        Window::new("Canvas Gradient Demo", CANVAS_WIDTH as i32, CANVAS_HEIGHT as i32)?;
    let mut canvas = Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(248, 248, 248));

    canvas.draw(|ctx| draw_gradient_scene(ctx))?;

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    main_box.add(canvas)?;
    window.set_content(main_box)?;
    window.run()?;

    Ok(())
}

fn draw_gradient_scene(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    let linear = LinearGradient {
        start: Point::new(50.0, 50.0),
        end: Point::new(250.0, 250.0),
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(255, 100, 100),
            },
            GradientStop {
                offset: 0.5,
                color: Color::rgb(255, 255, 150),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(100, 100, 255),
            },
        ],
    };
    ctx.fill_linear_gradient(&linear, Rect::new(20.0, 20.0, 260.0, 260.0))?;

    let radial = RadialGradient {
        center: Point::new(450.0, 150.0),
        radius: 100.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(200, 255, 200),
            },
            GradientStop {
                offset: 0.6,
                color: Color::rgb(80, 180, 80),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(20, 80, 20),
            },
        ],
    };
    ctx.fill_radial_gradient(&radial, Rect::new(350.0, 20.0, 200.0, 260.0))?;

    Ok(())
}
