//! Windows process-memory probe, not VRAM accounting or an FPS benchmark.
//! Compare `-- shared` and `-- isolated` in separate release-mode processes.

#[cfg(windows)]
mod probe {
    use aurea::{
        AureaResult, Window,
        render::{Canvas, Color, Paint, Point, Rect, RendererBackend, ZenGpuContext},
    };
    use std::{env, error::Error, ffi::c_void, io, mem::size_of, sync::Arc};

    #[repr(C)]
    #[derive(Default)]
    struct Memory {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged_pool: usize,
        paged_pool: usize,
        peak_nonpaged_pool: usize,
        nonpaged_pool: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(process: *mut c_void, counters: *mut Memory, size: u32) -> i32;
    }

    fn report(stage: &str) -> io::Result<()> {
        let size = u32::try_from(size_of::<Memory>()).expect("small ABI structure");
        let mut counters = Memory {
            cb: size,
            ..Memory::default()
        };
        // SAFETY: current-process pseudo handle and writable PROCESS_MEMORY_COUNTERS_EX layout.
        if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        println!(
            "{stage:<24} working set {:>7.1} MiB; private commit {:>7.1} MiB",
            counters.working_set as f64 / 1048576.0,
            counters.private_usage as f64 / 1048576.0
        );
        Ok(())
    }

    fn canvas(isolated: bool) -> AureaResult<(Window, Canvas)> {
        let mut window = Window::new("Vulkan memory probe", 320, 240)?;
        let mut canvas = if isolated {
            Canvas::with_gpu_context(320, 240, Arc::new(ZenGpuContext::new()?))?
        } else {
            Canvas::new(320, 240, RendererBackend::ZenGpu)?
        };
        window.set_content(canvas.clone())?;
        canvas.draw(|ctx| ctx.clear(Color::rgb(24, 26, 30)))?;
        Ok((window, canvas))
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mode = env::args().nth(1).unwrap_or_else(|| "shared".into());
        if mode != "shared" && mode != "isolated" && mode != "context" {
            return Err("use shared, isolated, or context".into());
        }
        println!("{mode} devices; counters differ from Task Manager's private working-set column");
        report("before window/GPU")?;
        if mode == "context" {
            let context = ZenGpuContext::new()?;
            report("device only, no surface")?;
            drop(context);
            report("after device closes")?;
            return Ok(());
        }
        let (first_window, mut first) = canvas(mode == "isolated")?;
        report("first canvas, no text")?;
        first.draw(|ctx| {
            ctx.draw_rect(Rect::new(5.0, 5.0, 100.0, 30.0), &Paint::new())?;
            ctx.draw_text("Aurea GPU memory", Point::new(8.0, 70.0), &Paint::new())
        })?;
        report("after first text")?;
        let (second_window, second) = canvas(mode == "isolated")?;
        report("second canvas")?;
        let first_context = first.gpu_context().ok_or("first GPU context missing")?;
        let second_context = second.gpu_context().ok_or("second GPU context missing")?;
        println!(
            "same device: {}",
            Arc::ptr_eq(&first_context, &second_context)
        );
        drop(first_context);
        drop(second_context);
        drop(second);
        drop(second_window);
        report("after second closes")?;
        drop(first);
        drop(first_window);
        report("after all close")?;
        Ok(())
    }
}

#[cfg(windows)]
use std::error::Error;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn Error>> {
    probe::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("gpu_memory measures Windows process RAM; run it on Windows.");
}
