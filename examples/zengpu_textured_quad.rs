//! ZenGPU G3 — textured quad with bindless descriptor indexing.
//!
//! Opens an aurea window, uploads a checkerboard texture, and renders it
//! fullscreen via ZenGPU's Vulkan backend.  The fragment shader samples from
//! a bindless array of 64 combined image samplers; the texture is at slot 0.
//!
//! Run with:
//!   cargo run --example zengpu_textured_quad

use aurea::{Window, WindowEvent};
use zengpu_hal::{
    DeviceRequest, FilterMode, Format, GpuAdapter, GpuDevice, PresentMode,
    SamplerDesc, SurfaceConfig, TextureDesc, TextureUsage, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;

const W: u32 = 800;
const H: u32 = 600;
const TEX_SIZE: u32 = 256;
const CELL: u32 = 32;

fn checkerboard() -> Vec<u8> {
    let mut pixels = vec![0u8; (TEX_SIZE * TEX_SIZE * 4) as usize];
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let checker = ((x / CELL) + (y / CELL)) % 2 == 0;
            let (r, g, b) = if checker { (220, 50, 50) } else { (30, 30, 30) };
            let i = ((y * TEX_SIZE + x) * 4) as usize;
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = 255;
        }
    }
    pixels
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::new("ZenGPU — Textured Quad (bindless slot 0)", W as i32, H as i32)?;

    let inst = VulkanInstance::new_with_surface()?;

    let adapter = inst
        .request_vulkan_adapter()
        .ok_or("no Vulkan adapter found")?;
    eprintln!("ZenGPU: {}", adapter.info().name);

    let device = adapter.open_with_surface(DeviceRequest::default())?;

    // Upload checkerboard texture.
    let tex = device.create_texture(TextureDesc {
        width: TEX_SIZE,
        height: TEX_SIZE,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        samples: 1,
    })?;
    device.upload_texture_data(tex, &checkerboard())?;

    let samp = device.create_sampler(SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Nearest,
        ..SamplerDesc::default()
    })?;

    let handles = WindowHandles::from_window(&window)
        .map_err(|e| format!("window handle unavailable: {e:?}"))?;
    let (w, h) = window.size();
    let config = SurfaceConfig {
        format: Format::Bgra8Unorm,
        width: w,
        height: h,
        present_mode: PresentMode::Fifo,
    };

    let surface = inst.create_textured_surface(&handles, &device, config, tex, samp)?;

    eprintln!("surface: {}×{} ({} images)", w, h, surface.image_count());

    'main: loop {
        for event in window.poll_events() {
            if matches!(event, WindowEvent::CloseRequested) {
                break 'main;
            }
        }

        let frame = surface.acquire_frame()?;
        surface.present_frame(frame)?;
    }

    // Explicit cleanup order: surface → sampler → texture → device.
    drop(surface);
    device.destroy_sampler(samp);
    device.destroy_texture(tex);

    Ok(())
}
