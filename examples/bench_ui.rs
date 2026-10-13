//! Frame cost of a widget-heavy canvas on the CPU rasterizer, with no
//! window: a grid of buttons, sliders, toggles and fields beside a long
//! list. Runs frames that change nothing, frames where the pointer moves
//! over the grid, and frames that scroll the list, and prints the time and
//! the heap allocations each frame takes. Run with:
//!
//! ```text
//! cargo run --release --example bench_ui
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use aurea::render::{CpuRasterizer, Renderer};
use aurea::ui::{Align, Button, ListView, Slider, TextField, Theme, Toggle, Ui, UiState};
use aurea::{AureaResult, Modifiers, MouseButtons, Rect, WindowEvent};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const FRAMES: u32 = 300;
const ROWS: usize = 12;
const COLS: usize = 4;

/// Counts allocations, to show which frames allocate.
struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Id {
    Button(usize),
    Slider(usize),
    Toggle(usize),
    Field(usize),
    List,
}

struct App {
    levels: Vec<f32>,
    on: Vec<bool>,
    names: Vec<String>,
    scroll: f32,
    picked: Option<usize>,
}

fn panel(ui: &mut Ui<'_, Id>, app: &mut App) -> AureaResult<()> {
    let cell = |r: usize, c: usize| {
        #[allow(clippy::cast_precision_loss)]
        Rect::new(16.0 + c as f32 * 210.0, 16.0 + r as f32 * 60.0, 196.0, 30.0)
    };
    for r in 0..ROWS {
        for c in 0..COLS {
            let i = r * COLS + c;
            let at = cell(r, c);
            match c {
                0 => {
                    Button::new("Apply").show(ui, Id::Button(i), at)?;
                }
                1 => {
                    Slider::new(&mut app.levels[i], 0.0..=1.0).show(ui, Id::Slider(i), at)?;
                }
                2 => {
                    Toggle::new(&mut app.on[i])
                        .label("Enabled")
                        .show(ui, Id::Toggle(i), at)?;
                }
                _ => {
                    TextField::new(&mut app.names[i]).placeholder("Name").show(
                        ui,
                        Id::Field(i),
                        at,
                    )?;
                }
            }
        }
    }
    let list = Rect::new(870.0, 16.0, 390.0, 760.0);
    ListView::new(&mut app.scroll, 2000, 26.0)
        .selected(&mut app.picked)
        .show(ui, Id::List, list, |ui, row| {
            if row.picked || row.hovered {
                ui.fill(row.rect, ui.theme.palette.hover)?;
            }
            let fonts = ui.fonts();
            let text = Rect::new(
                row.rect.x + 10.0,
                row.rect.y,
                row.rect.width - 20.0,
                row.rect.height,
            );
            ui.text(
                "A row in a long list",
                text,
                &fonts.text,
                ui.theme.palette.text,
                Align::Start,
            )
        })?;
    Ok(())
}

fn run(name: &str, event: impl Fn(u32) -> Option<WindowEvent>) -> AureaResult<()> {
    let theme = Theme::dark();
    let mut raster = CpuRasterizer::new(WIDTH, HEIGHT);
    let mut state = UiState::<Id>::new();
    let n = ROWS * COLS;
    let mut app = App {
        levels: vec![0.5; n],
        on: vec![false; n],
        names: (0..n).map(|i| format!("Item {i}")).collect(),
        scroll: 0.0,
        picked: None,
    };
    let mut frame = |state: &mut UiState<Id>, app: &mut App| -> AureaResult<(f64, f64)> {
        let start = Instant::now();
        let mut ctx = raster.begin_frame()?;
        ctx.clear(theme.palette.background)?;
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            panel(&mut ui, app)?;
        }
        drop(ctx);
        let built = start.elapsed().as_secs_f64();
        raster.end_frame()?;
        Ok((built, start.elapsed().as_secs_f64()))
    };
    // Warm up: caches and buffers grow to their size.
    for _ in 0..30 {
        frame(&mut state, &mut app)?;
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let (mut built, mut total) = (0.0, 0.0);
    for i in 0..FRAMES {
        if let Some(event) = event(i) {
            state.handle(&event);
        }
        let (b, t) = frame(&mut state, &mut app)?;
        built += b;
        total += t;
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    let per = f64::from(FRAMES);
    println!(
        "{name:<8} {:>8.1} us/frame ({:>7.1} us building), {:>7.1} allocations/frame",
        total / per * 1e6,
        built / per * 1e6,
        allocations as f64 / per
    );
    Ok(())
}

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::MouseMove {
        x,
        y,
        buttons: MouseButtons::default(),
        modifiers: Modifiers::default(),
        time: Default::default(),
    }
}

fn main() -> AureaResult<()> {
    run("idle", |_| None)?;
    run("hover", |i| {
        let i = f64::from(i % 200);
        Some(moved(20.0 + i * 4.0, 20.0 + (i * 3.3) % 700.0))
    })?;
    run("scroll", |i| {
        Some(if i == 0 {
            moved(1000.0, 400.0)
        } else {
            WindowEvent::MouseWheel {
                delta_x: 0.0,
                delta_y: -1.0,
                modifiers: Modifiers::default(),
            }
        })
    })?;
    Ok(())
}
