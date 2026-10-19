//! The `ANativeWindow` a `Surface` hands over, and CPU frames posted to it.

use std::os::raw::c_void;
use std::ptr::{NonNull, null_mut};
use std::slice::from_raw_parts_mut;

use jni::JNIEnv;
use jni::objects::JObject;

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
    fn ANativeWindow_lock(window: *mut c_void, buffer: *mut Buffer, dirty: *mut c_void) -> i32;
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

    pub(super) fn post(&self, pixels: &[u32], width: u32, height: u32) -> bool {
        let mut buffer = Buffer {
            width: 0,
            height: 0,
            stride: 0,
            format: 0,
            bits: null_mut(),
            reserved: [0; 6],
        };
        let window = self.0.as_ptr();
        if unsafe { ANativeWindow_lock(window, &raw mut buffer, null_mut()) } != 0 {
            return false;
        }
        let rows = height.min(u32::try_from(buffer.height).unwrap_or(0)) as usize;
        let cols = width.min(u32::try_from(buffer.width).unwrap_or(0)) as usize;
        let stride = usize::try_from(buffer.stride).unwrap_or(0);
        if buffer.format == RGBA_8888 && !buffer.bits.is_null() && stride >= cols {
            // SAFETY: the locked buffer holds `height` rows of `stride` pixels.
            let dst = unsafe {
                from_raw_parts_mut(
                    buffer.bits.cast::<u32>(),
                    stride * usize::try_from(buffer.height).unwrap_or(0),
                )
            };
            for y in 0..rows {
                let src = &pixels[y * width as usize..][..cols];
                for (d, &p) in dst[y * stride..][..cols].iter_mut().zip(src) {
                    *d = (p & 0xFF00_FF00) | ((p >> 16) & 0xFF) | ((p & 0xFF) << 16);
                }
            }
        }
        unsafe { ANativeWindow_unlockAndPost(window) == 0 }
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe { ANativeWindow_release(self.0.as_ptr()) };
    }
}
