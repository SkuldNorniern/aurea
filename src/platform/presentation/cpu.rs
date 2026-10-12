//! Handing a CPU-drawn frame to the native canvas: copied into a surface the platform shares when
//! it offers one, otherwise left for its next paint to blit.

use std::os::raw::c_void;
use std::ptr::copy_nonoverlapping;

use aurea_render::Rect;

use crate::ffi::*;

fn f32_to_usize_clamped(v: f32) -> usize {
    let v = v.clamp(0.0, 16_777_216.0);
    // SAFETY: v is clamped to a non-negative, exactly-representable range above.
    unsafe { v.to_int_unchecked() }
}

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

    let src = ptr as *const u32;
    let dst = dst as *mut u32;
    unsafe {
        match refresh {
            None => {
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
    }
}
