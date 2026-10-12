//! Making the renderer a canvas draws with, when it has none yet.

use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::Rc;

#[cfg(feature = "zengpu")]
use crate::AureaError;
use crate::AureaResult;
#[cfg(feature = "zengpu")]
use crate::ffi::*;
#[cfg(all(feature = "zengpu", target_os = "macos"))]
use crate::platform::handles::NativeWindowHandle;
#[cfg(feature = "zengpu")]
use crate::platform::handles::native_handle_from_canvas_ptr;
#[cfg(feature = "zengpu")]
use crate::platform::zengpu::window_handles;
#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuRenderer;
use aurea_render::{Renderer, RendererBackend};
#[cfg(feature = "zengpu")]
use zengpu_hal::WindowHandles;

use super::state::CanvasState;

#[cfg(feature = "zengpu")]
pub(super) fn ensure_canvas_renderer(
    handle: *mut c_void,
    state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
) -> AureaResult<bool> {
    if renderer.borrow_mut().is_some() {
        return Ok(true);
    }
    if backend != RendererBackend::ZenGpu {
        return Ok(false);
    }

    let window = unsafe { ng_platform_canvas_get_window(handle) };
    if window.is_null() || unsafe { ng_platform_canvas_get_native_handle(handle) }.is_null() {
        return Ok(false);
    }

    let handles = zengpu_canvas_handles(handle)?;
    let (width, height, scale_factor) = {
        let st = state.borrow_mut();
        (st.width.max(1), st.height.max(1), st.scale_factor.max(1.0))
    };
    let gpu = ZenGpuRenderer::new(&handles, width, height, scale_factor)?;
    *renderer.borrow_mut() = Some(Box::new(gpu));
    Ok(true)
}

#[cfg(not(feature = "zengpu"))]
pub(super) fn ensure_canvas_renderer(
    _handle: *mut c_void,
    _state: &Rc<RefCell<CanvasState>>,
    renderer: &Rc<RefCell<Option<Box<dyn Renderer>>>>,
    _backend: RendererBackend,
) -> AureaResult<bool> {
    Ok(renderer.borrow_mut().is_some())
}

/// Native handle extraction differs from [`native_handle_from_canvas_ptr`](crate::platform::handles::native_handle_from_canvas_ptr) on
/// macOS: the ZenGPU surface needs the resolved `NSView` from
/// `ng_platform_canvas_get_native_handle`, not the raw canvas handle.
#[cfg(feature = "zengpu")]
fn zengpu_canvas_handles(handle: *mut c_void) -> AureaResult<WindowHandles> {
    #[cfg(target_os = "macos")]
    {
        let view = unsafe { ng_platform_canvas_get_native_handle(handle) };
        let native = NativeWindowHandle::MacOS { ns_view: view };
        window_handles(&native)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let native =
            native_handle_from_canvas_ptr(handle).ok_or(AureaError::ElementOperationFailed)?;
        window_handles(&native)
    }
}
