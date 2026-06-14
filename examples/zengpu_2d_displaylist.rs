//! ZenGPU G4 / Rung 1 — the full `DisplayList → ZenGPU` pipeline.
//!
//! Drives `ZenGpuRenderer` (an aurea `Renderer`) the same way the framework
//! would: `begin_frame` hands back a `DrawingContext`, draw calls are recorded
//! into a display list, and `end_frame` lowers that list to instanced rects and
//! presents them through ZenGPU's Vulkan backend.
//!
//! Run with:
//!   cargo run --example zengpu_2d_displaylist --features zengpu

use aurea::render::{
    Color, GradientStop, Image, LinearGradient, Paint, Point, RadialGradient, Rect, Renderer,
};
use aurea::{Window, WindowEvent};

const W: i32 = 800;
const H: i32 = 600;

fn paint(r: u8, g: u8, b: u8, a: u8) -> Paint {
    Paint::new().color(Color::rgba(r, g, b, a))
}

fn test_image() -> Image {
    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 64;
    let mut data = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            let color = if ((x / 8) + (y / 8)) % 2 == 0 {
                [245, 245, 255, 255]
            } else {
                [65, 80, 145, 255]
            };
            data[i..i + 4].copy_from_slice(&color);
        }
    }
    Image::new(WIDTH, HEIGHT, data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — DisplayList 2D (G4 Rung 1)", W, H)?;

    // Window-level GPU path: the swapchain belongs to the window.
    let mut renderer = window.create_zengpu_2d()?;
    eprintln!(
        "ZenGPU 2D surface: {}×{}",
        renderer.size().0,
        renderer.size().1
    );

    let image = test_image();
    let mut frame: u64 = 0;
    'main: loop {
        for event in window.poll_events() {
            match event {
                WindowEvent::CloseRequested => break 'main,
                WindowEvent::Resized { width, height } => {
                    renderer.resize(width, height)?;
                }
                _ => {}
            }
        }

        // Programmatically resize once to exercise swapchain recreation.
        frame += 1;
        if frame == 120 {
            window.set_size(1000, 700);
        }

        let mut ctx = renderer.begin_frame()?;
        ctx.clear(Color::rgb(20, 20, 28))?;
        ctx.draw_rect(
            Rect::new(40.0, 40.0, 200.0, 150.0),
            &paint(220, 50, 50, 255),
        )?;
        ctx.draw_rect(
            Rect::new(280.0, 40.0, 200.0, 150.0),
            &paint(50, 200, 80, 255),
        )?;
        ctx.draw_rect(
            Rect::new(520.0, 40.0, 200.0, 150.0),
            &paint(60, 120, 230, 255),
        )?;
        ctx.draw_rect(
            Rect::new(40.0, 230.0, 680.0, 120.0),
            &paint(240, 200, 40, 255),
        )?;
        // Translucent white panel exercises the alpha-blend path.
        ctx.draw_rect(
            Rect::new(120.0, 120.0, 480.0, 260.0),
            &paint(255, 255, 255, 110),
        )?;
        // Antialiased filled circles (Rung 2 SDF circle path).
        ctx.draw_circle(Point::new(240.0, 440.0), 60.0, &paint(255, 120, 160, 255))?;
        ctx.draw_circle(Point::new(440.0, 440.0), 80.0, &paint(120, 220, 255, 200))?;
        // LUT-sampled gradients, including a three-stop linear gradient.
        ctx.fill_linear_gradient(
            &LinearGradient {
                start: Point::new(560.0, 360.0),
                end: Point::new(760.0, 360.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb(230, 60, 60),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: Color::rgb(70, 220, 120),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(60, 90, 230),
                    },
                ],
            },
            Rect::new(560.0, 360.0, 200.0, 180.0),
        )?;
        ctx.fill_radial_gradient(
            &RadialGradient {
                center: Point::new(120.0, 450.0),
                radius: 90.0,
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb(250, 240, 140),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(40, 40, 60),
                    },
                ],
            },
            Rect::new(30.0, 360.0, 180.0, 180.0),
        )?;
        // Reusing this Arc-backed image every frame exercises the upload-once
        // texture cache as well as full-image and source-region UVs.
        ctx.draw_image_rect(&image, Rect::new(300.0, 390.0, 128.0, 128.0))?;
        ctx.draw_image_region(
            &image,
            Rect::new(16.0, 16.0, 32.0, 32.0),
            Rect::new(445.0, 415.0, 80.0, 80.0),
        )?;
        ctx.draw_text(
            "Aurea GPU text",
            Point::new(275.0, 570.0),
            &paint(245, 245, 255, 255),
        )?;
        drop(ctx);
        renderer.end_frame()?;
    }

    Ok(())
}
