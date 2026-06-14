//! ZenGPU G4 / Rung 1 — instanced 2D rectangles through the Vulkan painter.
//!
//! Opens an aurea window and presents a batch of solid-colour rects each frame
//! via `Vulkan2dSurface`, the backend the aurea `ZenGpuRenderer` will drive.
//! This is the standalone proof of the painter before it's threaded through
//! aurea's `Renderer` abstraction.
//!
//! Run with:
//!   cargo run --example zengpu_2d_rects

use aurea::{Window, WindowEvent};
use zengpu_hal::{DeviceRequest, Format, GpuAdapter, PresentMode, SurfaceConfig, WindowHandles};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{CircleInstance, Frame2d, RectInstance};

const W: i32 = 800;
const H: i32 = 600;

fn rgba(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

fn scene() -> Vec<RectInstance> {
    vec![
        // A grid of opaque rects plus one translucent overlay.
        RectInstance {
            rect: [40.0, 40.0, 200.0, 150.0],
            color: rgba(220, 50, 50, 255),
        },
        RectInstance {
            rect: [280.0, 40.0, 200.0, 150.0],
            color: rgba(50, 200, 80, 255),
        },
        RectInstance {
            rect: [520.0, 40.0, 200.0, 150.0],
            color: rgba(60, 120, 230, 255),
        },
        RectInstance {
            rect: [40.0, 230.0, 680.0, 120.0],
            color: rgba(240, 200, 40, 255),
        },
        // Translucent white panel over everything.
        RectInstance {
            rect: [120.0, 120.0, 480.0, 260.0],
            color: rgba(255, 255, 255, 110),
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — 2D Rects (G4 Rung 1)", W, H)?;

    let inst = VulkanInstance::new_with_surface()?;
    let adapter = inst
        .request_vulkan_adapter()
        .ok_or("no Vulkan adapter found")?;
    eprintln!("ZenGPU: {}", adapter.info().name);

    let device = adapter.open_with_surface(DeviceRequest::default())?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle unavailable: {e:?}"))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };

    let surface = inst.create_2d_surface(&handles, &device, config)?;
    eprintln!(
        "surface: {}×{} ({} images)",
        surface.size().0,
        surface.size().1,
        surface.image_count()
    );

    let rects = scene();
    let circles = vec![
        // Two filled circles to exercise the SDF circle pipeline.
        CircleInstance {
            center_radius: [200.0, 440.0, 60.0, 0.0],
            color: rgba(255, 120, 160, 255),
        },
        CircleInstance {
            center_radius: [400.0, 440.0, 80.0, 0.0],
            color: rgba(120, 220, 255, 200),
        },
    ];
    let clear = Some(rgba(20, 20, 28, 255));

    'main: loop {
        for event in window.poll_events() {
            if matches!(event, WindowEvent::CloseRequested) {
                break 'main;
            }
        }
        surface.present(Frame2d {
            clear,
            rects: &rects,
            gradients: &[],
            images: &[],
            circles: &circles,
        })?;
    }

    drop(surface);
    Ok(())
}
