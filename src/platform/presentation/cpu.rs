//! Handing a CPU-drawn frame to the native canvas: copied into a surface
//! the platform shares when it offers one, otherwise left for its next
//! paint to blit.

use std::os::raw::c_void;
use std::ptr::copy_nonoverlapping;

use aurea_render::Rect;

use crate::ffi::*;

/// Converts a non-negative, pre-clamped `f32` row coordinate to `usize`.
/// `std` has no safe non-`as` float-to-int conversion; clippy's
/// `cast_possible_truncation`/`cast_sign_loss` fire on `as usize` unconditionally,
/// so this isolates the unchecked conversion behind the caller's clamp.
fn f32_to_usize_clamped(v: f32) -> usize {
    let v = v.clamp(0.0, 16_777_216.0);
    // SAFETY: v is clamped to a non-negative, exactly-representable range above.
    unsafe { v.to_int_unchecked() }
}

/// Publish a freshly-rendered CPU frame to the platform.
///
/// `refresh` is the physical-pixel rect that needs to be updated this present
/// (union of current-frame damage and the previous frame's damage, to account
/// for the IOSurface double-buffer being 2 frames stale). `None` = full frame.
///
/// **macOS zero-copy path** (`ng_platform_canvas_acquire_buffer` returns non-NULL):
/// Locks the IOSurface back buffer, copies only the `refresh` rows from the
/// rasterizer's frame_buffer into it (CPU→shared-memory, not a GPU upload), then
/// `ng_platform_canvas_present` flips the surface onto the layer. No additional
/// invalidate is needed — CoreAnimation picks up the layer change immediately.
///
/// **Legacy fallback** (acquire returns NULL — Windows, Linux today):
/// Stores the Rust buffer pointer via `ng_platform_canvas_update_buffer` and
/// issues a damage-aware platform repaint (`invalidate_rect` when `refresh` is
/// known, `invalidate` for a full redraw). The platform's paint handler blits the
/// buffer to the screen on the next paint event.
///
/// # Safety
/// `ptr` must point to at least `w * h` packed `u32` pixels and stay valid for the
/// duration of this call. Must be called on the platform's main/UI thread.
pub(crate) unsafe fn publish_cpu_buffer(
    handle: *mut c_void,
    ptr: *const u8,
    size: usize,
    w: u32,
    h: u32,
    refresh: Option<Rect>,
) {
    let mut stride_px: u32 = 0;
    let mut buffer_index: u32 = 0;
    let dst = unsafe {
        ng_platform_canvas_acquire_buffer(handle, w, h, &mut stride_px, &mut buffer_index)
    };

    if dst.is_null() {
        // Legacy path: store Rust buffer pointer, let the platform blit on repaint.
        unsafe {
            ng_platform_canvas_update_buffer(
                handle,
                ptr,
                u32::try_from(size).expect("buffer size fits in u32"),
                w,
                h,
            );
            match refresh {
                Some(r) => {
                    ng_platform_canvas_invalidate_rect(handle, r.x, r.y, r.width, r.height);
                }
                None => {
                    ng_platform_canvas_invalidate(handle);
                }
            }
        }
        return;
    }

    // Zero-copy present: copy refresh rows from frame_buffer into the IOSurface.
    // stride_px may exceed w due to IOSurface row alignment — copy row-by-row.
    let src = ptr as *const u32;
    let dst = dst as *mut u32;
    unsafe {
        match refresh {
            None => {
                // Full frame: copy all rows.
                if stride_px == w {
                    copy_nonoverlapping(src, dst, w as usize * h as usize);
                } else {
                    for y in 0..h as usize {
                        copy_nonoverlapping(
                            src.add(y * w as usize),
                            dst.add(y * stride_px as usize),
                            w as usize,
                        );
                    }
                }
            }
            Some(r) => {
                // Partial: only rows that intersect the refresh rect.
                let y0 = f32_to_usize_clamped(r.y.floor().max(0.0));
                let y1 = f32_to_usize_clamped((r.y + r.height).ceil().min(h as f32));
                for y in y0..y1 {
                    copy_nonoverlapping(
                        src.add(y * w as usize),
                        dst.add(y * stride_px as usize),
                        w as usize,
                    );
                }
            }
        }
        ng_platform_canvas_present(handle);
        // IOSurface present: CoreAnimation picks up the change immediately;
        // no additional invalidate call is needed.
    }
}
