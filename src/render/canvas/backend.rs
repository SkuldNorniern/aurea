//! Making the renderer a canvas draws with, when it has none yet.

use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::Rc;

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
use aurea_render::{Renderer, RendererBackend};
#[cfg(feature = "zengpu")]
use aurea_render::{ZenGpuContext, ZenGpuRenderer};
#[cfg(feature = "zengpu")]
use std::sync::Arc;
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
    if renderer
        .try_borrow()
        .map_err(|_| AureaError::RenderingFailed)?
        .is_some()
    {
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
    let (width, height, scale_factor, context) = {
        let st = state.borrow_mut();
        (
            st.width.max(1),
            st.height.max(1),
            st.scale_factor.max(1.0),
            st.gpu_context.clone(),
        )
    };
    let context = match context {
        Some(context) => context,
        None => Arc::new(ZenGpuContext::new()?),
    };
    let gpu =
        ZenGpuRenderer::with_context(&handles, Arc::clone(&context), width, height, scale_factor)?;
    state.borrow_mut().gpu_context = Some(context);
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
    Ok(renderer
        .try_borrow()
        .map_err(|_| AureaError::RenderingFailed)?
        .is_some())
}

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
