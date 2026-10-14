//! Main-thread native checks and a visual surface for the Lunia UI gaps.

use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::thread;

use aurea::clipboard::ClipboardImage;
use aurea::elements::{Orientation, Stack};
#[cfg(not(feature = "zengpu"))]
use aurea::render::RendererBackend;
#[cfg(feature = "zengpu")]
use aurea::render::ZenGpuContext;
use aurea::render::{Canvas, Color, Image, Rect, Sampling, Transform, request_canvas_redraw};
use aurea::ui::{Dock, DockSide, DockTree, DockView, Dropdown, DropdownStyle, Theme, Ui, UiState};
use aurea::{
    AureaResult, Container, CursorIcon, MenuShortcut, ShortcutKey, Window, WindowEvent,
    reduced_motion,
};
#[cfg(feature = "zengpu")]
use std::sync::Arc;

fn native_checks(window: &mut Window) -> AureaResult<()> {
    let root = window.create_menu_bar()?;
    let mut menu = root.add_submenu("Layers")?;
    for _ in 0..300 {
        let mut child = menu.add_submenu("Adjustment")?;
        let mut nested = child.add_submenu("More")?;
        let item = nested.add_item("Levels", || {})?;
        menu.clear()?;
        assert!(child.add_separator().is_err());
        assert!(nested.clear().is_err());
        assert!(item.set_enabled(false).is_err());
    }
    let mut adjustment = menu.add_submenu("Adjustment")?;
    adjustment.add_item_with_shortcut(
        "Levels",
        MenuShortcut::new(ShortcutKey::Char('L')).primary(),
        || println!("Levels selected once"),
    )?;
    let disabled = adjustment.add_item("Disabled", || eprintln!("BUG: disabled item ran"))?;
    disabled.set_enabled(false)?;
    let image = Image::new(24, 24, [255, 255, 255, 128].repeat(24 * 24));
    for _ in 0..300 {
        window.set_cursor_image(&image, (12, 12))?;
        window.set_cursor(CursorIcon::ResizeNwSe)?;
        window.set_cursor(CursorIcon::ResizeNeSw)?;
    }
    window.set_cursor_image(&image, (12, 12))?;
    thread::spawn(|| {
        let image = ClipboardImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 128, 0, 0, 255, 255],
        };
        assert_eq!(
            ClipboardImage::from_png(&image.to_png().expect("PNG"), 4096).expect("decode PNG"),
            image
        );
        assert_eq!(
            ClipboardImage::from_dib(&image.to_dib().expect("DIB"), 4096).expect("decode DIB"),
            image
        );
    })
    .join()
    .expect("worker codecs");
    println!("native menu/cursor lifecycle and worker codecs passed");
    Ok(())
}

struct State {
    ui: UiState<u8>,
    dock: Dock<u8>,
    sampling: usize,
}

fn pattern() -> Image {
    let mut pixels = Vec::with_capacity(257 * 129 * 4);
    for y in 0..129 {
        for x in 0..257 {
            pixels.extend_from_slice(if x == 256 {
                &[255, 0, 255, 255]
            } else if x % 8 == 0 || y % 8 == 0 {
                &[255, 255, 255, 255]
            } else {
                &[24, 28, 36, 255]
            });
        }
    }
    Image::new(257, 129, pixels)
}

fn main() -> AureaResult<()> {
    let mut window = Window::new(
        "Native gaps: menus, cursors, popup and dock motion",
        800,
        500,
    )?;
    native_checks(&mut window)?;
    if env::args().any(|arg| arg == "--check") {
        return Ok(());
    }
    let image = pattern();
    #[cfg(feature = "zengpu")]
    let context = Arc::new(ZenGpuContext::new()?);
    #[cfg(feature = "zengpu")]
    let owned = context.upload_image(&image, Sampling::Smooth)?;
    #[cfg(feature = "zengpu")]
    let canvas = Canvas::with_gpu_context(800, 500, context)?;
    #[cfg(not(feature = "zengpu"))]
    let canvas = Canvas::new(800, 500, RendererBackend::Cpu)?;
    let id = canvas.id();
    let mut tree = DockTree::new(0);
    tree.insert(1, 0, DockSide::Center);
    let mut ui = UiState::new();
    ui.set_reduced_motion(reduced_motion());
    let state = Rc::new(RefCell::new(State {
        ui,
        dock: Dock::new(tree),
        sampling: 2,
    }));
    let drawn = Rc::clone(&state);
    canvas.set_draw_callback(move |ctx| {
        let theme = Theme::dark();
        ctx.clear(theme.palette.background)?;
        #[allow(clippy::cast_precision_loss)]
        let width = ctx.width() as f32;
        #[allow(clippy::cast_precision_loss)]
        let height = ctx.height() as f32;
        let State { ui, dock, sampling } = &mut *drawn.borrow_mut();
        {
            let mut ui = ui.frame(ctx, &theme);
            let mut style = DropdownStyle::new(&theme);
            style.open_border = Color::rgb(160, 160, 160);
            style.popup.check = Color::rgb(200, 200, 200);
            style.popup.highlight = Color::rgb(64, 64, 64);
            Dropdown::new(&["Nearest", "Linear", "Smooth"], sampling)
                .style(style)
                .show(&mut ui, 1, Rect::new(16.0, 16.0, 200.0, 32.0))?;
            let sampling = [Sampling::Nearest, Sampling::Linear, Sampling::Smooth][*sampling];
            DockView::new(
                dock,
                |p| {
                    if p == 0 {
                        "Pixels / 한글"
                    } else {
                        "Shared texture"
                    }
                },
                |ui: &mut Ui<'_, u8>, panel, rect: Rect| {
                    ui.ctx.save()?;
                    ui.ctx.set_sampling(sampling)?;
                    ui.ctx.transform(
                        Transform::rotate(0.25)
                            .multiply(Transform::translate(rect.x + 100.0, rect.y + 60.0)),
                    )?;
                    #[cfg(feature = "zengpu")]
                    if panel == 1 {
                        ui.ctx.draw_texture(
                            owned.texture(),
                            Rect::new(0.0, 0.0, 1.0, 1.0),
                            Rect::new(0.0, 0.0, 120.0, 60.0),
                        )?;
                    } else {
                        ui.ctx
                            .draw_image_rect(&image, Rect::new(0.0, 0.0, 120.0, 60.0))?;
                    }
                    #[cfg(not(feature = "zengpu"))]
                    {
                        let _ = panel;
                        ui.ctx
                            .draw_image_rect(&image, Rect::new(0.0, 0.0, 120.0, 60.0))?;
                    }
                    ui.ctx.restore()?;
                    Ok(())
                },
            )
            .close_buttons(true)
            .show(
                &mut ui,
                2,
                Rect::new(
                    16.0,
                    64.0,
                    (width - 32.0).max(0.0),
                    (height - 80.0).max(0.0),
                ),
            )?;
            for event in dock.take_tab_events() {
                println!("dock: {event:?}");
            }
        }
        if ui.wants_frame() {
            request_canvas_redraw(id);
        }
        Ok(())
    })?;
    let input = Rc::clone(&state);
    window.on_event(move |event| {
        if matches!(
            event,
            WindowEvent::KeyInput { pressed: true, .. }
                | WindowEvent::Pen(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            println!("input: {event:?}");
        }
        if input.borrow_mut().ui.handle(&event) || matches!(event, WindowEvent::Resized { .. }) {
            request_canvas_redraw(id);
        }
    });
    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    window.show();
    window.run()
}
