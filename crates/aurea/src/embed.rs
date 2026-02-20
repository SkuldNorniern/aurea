//! Embed FFI for Aurea-in-SwiftUI (macOS) and similar host integration.
//!
//! Exports C-callable functions so a Swift/ObjC app can create an Aurea canvas view
//! and embed it via NSViewRepresentable.

use crate::ffi::*;
use crate::render::{Canvas, Color, Paint, PaintStyle, Point, Rect, RendererBackend};
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::{c_int, c_void};

thread_local! {
    static EMBED_CANVASES: RefCell<HashMap<usize, Box<Canvas>>> = RefCell::new(HashMap::new());
    static EMBED_INIT: RefCell<bool> = RefCell::new(false);
}

fn ensure_init() {
    EMBED_INIT.with(|init| {
        let mut flag = init.borrow_mut();
        if !*flag {
            let _ = unsafe { ng_platform_init() };
            *flag = true;
        }
    });
}

/// Create an embeddable canvas view. Returns the native NSView* (macOS) or equivalent.
/// The caller (Swift/ObjC) adds this view to its hierarchy. Canvas is retained by Aurea.
#[unsafe(no_mangle)]
pub extern "C" fn aurea_embed_create_canvas(width: c_int, height: c_int) -> *mut c_void {
    if width <= 0 || height <= 0 {
        return std::ptr::null_mut();
    }
    ensure_init();

    let mut canvas = match Canvas::new(width as u32, height as u32, RendererBackend::Cpu) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let _ = canvas.draw(|ctx| {
        ctx.clear(Color::rgb(248, 248, 252))?;
        let paint = Paint::new()
            .color(Color::rgb(100, 100, 120))
            .style(PaintStyle::Fill);
        ctx.draw_rect(
            Rect::new(
                (width as f32) * 0.2,
                (height as f32) * 0.4,
                (width as f32) * 0.6,
                (height as f32) * 0.2,
            ),
            &paint,
        )?;
        ctx.draw_circle(
            Point::new((width as f32) * 0.5, (height as f32) * 0.5),
            (width as f32) * 0.05,
            &paint,
        )?;
        Ok(())
    });
    let ptr = canvas.native_handle();
    let key = ptr as usize;
    EMBED_CANVASES.with(|c| {
        c.borrow_mut().insert(key, Box::new(canvas));
    });
    ptr
}

/// Release an embed canvas. Call when the host view is removed.
/// After this, the handle must not be used.
#[unsafe(no_mangle)]
pub extern "C" fn aurea_embed_destroy_canvas(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let key = handle as usize;
    EMBED_CANVASES.with(|c| {
        c.borrow_mut().remove(&key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_create_returns_non_null_on_valid_size() {
        let handle = aurea_embed_create_canvas(100, 100);
        assert!(!handle.is_null());
        aurea_embed_destroy_canvas(handle);
    }

    #[test]
    fn embed_create_returns_null_on_invalid_size() {
        assert!(aurea_embed_create_canvas(0, 100).is_null());
        assert!(aurea_embed_create_canvas(100, 0).is_null());
        assert!(aurea_embed_create_canvas(-1, 100).is_null());
    }
}
