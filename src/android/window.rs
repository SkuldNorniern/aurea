//! The `ANativeWindow` a `Surface` hands over, and CPU frames posted to it.

use std::os::raw::c_void;
use std::ptr::{NonNull, null_mut};
use std::slice::from_raw_parts_mut;

use jni::JNIEnv;
use jni::objects::JObject;

use crate::Rect;

const RGBA_8888: i32 = 1;

#[repr(C)]
struct Buffer {
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    bits: *mut c_void,
    reserved: [u32; 6],
}

/// `ARect`: a dirty region in buffer pixels, right and bottom exclusive.
#[repr(C)]
struct DirtyRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_fromSurface(env: *mut c_void, surface: *mut c_void) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
    fn ANativeWindow_setBuffersGeometry(
        window: *mut c_void,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;
    fn ANativeWindow_lock(window: *mut c_void, buffer: *mut Buffer, dirty: *mut DirtyRect) -> i32;
    fn ANativeWindow_unlockAndPost(window: *mut c_void) -> i32;
}

pub(super) struct NativeWindow(NonNull<c_void>);

impl NativeWindow {
    pub(super) fn from_surface(
        env: &JNIEnv,
        surface: &JObject,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let raw =
            unsafe { ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast()) };
        let window = Self(NonNull::new(raw)?);
        let (w, h) = (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        let set = unsafe { ANativeWindow_setBuffersGeometry(window.0.as_ptr(), w, h, RGBA_8888) };
        (set == 0).then_some(window)
    }

    /// Copies `damage` of the frame, or all of it, into the next buffer and posts it. Android
    /// keeps the rest from the last buffer, and may widen the region to what that needs.
    pub(super) fn post(
        &self,
        pixels: &[u32],
        width: u32,
        height: u32,
        damage: Option<Rect>,
    ) -> bool {
        let mut buffer = Buffer {
            width: 0,
            height: 0,
            stride: 0,
            format: 0,
            bits: null_mut(),
            reserved: [0; 6],
        };
        let whole = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        let mut dirty = damage.map_or(
            DirtyRect {
                left: 0,
                top: 0,
                right: whole(width),
                bottom: whole(height),
            },
            |r| DirtyRect {
                left: pixel(r.x.floor()),
                top: pixel(r.y.floor()),
                right: pixel(r.right().ceil()),
                bottom: pixel(r.bottom().ceil()),
            },
        );
        let window = self.0.as_ptr();
        if unsafe { ANativeWindow_lock(window, &raw mut buffer, &raw mut dirty) } != 0 {
            return false;
        }
        let span = |lo: i32, hi: i32, size: u32, held: i32| {
            let end = size
                .min(u32::try_from(held).unwrap_or(0))
                .min(u32::try_from(hi).unwrap_or(0));
            (
                u32::try_from(lo).unwrap_or(0).min(end) as usize,
                end as usize,
            )
        };
        let (y0, y1) = span(dirty.top, dirty.bottom, height, buffer.height);
        let (x0, x1) = span(dirty.left, dirty.right, width, buffer.width);
        let stride = usize::try_from(buffer.stride).unwrap_or(0);
        if buffer.format == RGBA_8888 && !buffer.bits.is_null() && stride >= x1 {
            // SAFETY: the locked buffer holds `height` rows of `stride` pixels.
            let dst = unsafe {
                from_raw_parts_mut(
                    buffer.bits.cast::<u32>(),
                    stride * usize::try_from(buffer.height).unwrap_or(0),
                )
            };
            for y in y0..y1 {
                let src = &pixels[y * width as usize..][x0..x1];
                for (d, &p) in dst[y * stride..][x0..x1].iter_mut().zip(src) {
                    *d = (p & 0xFF00_FF00) | ((p >> 16) & 0xFF) | ((p & 0xFF) << 16);
                }
            }
        }
        unsafe { ANativeWindow_unlockAndPost(window) == 0 }
    }
}

/// A pixel edge, saturating; `as` maps NaN to 0.
#[allow(clippy::cast_possible_truncation)]
fn pixel(v: f32) -> i32 {
    v as i32
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe { ANativeWindow_release(self.0.as_ptr()) };
    }
}
