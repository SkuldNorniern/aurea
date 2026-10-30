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
#[cfg(all(feature = "zengpu", not(target_os = "macos")))]
use crate::platform::handles::native_handle_from_canvas_ptr;
#[cfg(feature = "zengpu")]
use crate::platform::zengpu::window_handles;
use aurea_render::{Renderer, RendererBackend};
#[cfg(feature = "zengpu")]
use aurea_render::{ZenGpuContext, ZenGpuRenderer};
#[cfg(feature = "zengpu")]
use std::sync::{Arc, Weak};
#[cfg(feature = "zengpu")]
use zengpu_hal::WindowHandles;

use super::state::CanvasState;

#[cfg(feature = "zengpu")]
thread_local! {
    static DEFAULT_GPU_CONTEXT: RefCell<Weak<ZenGpuContext>> = const { RefCell::new(Weak::new()) };
}

#[cfg(feature = "zengpu")]
pub(super) fn default_gpu_context(adapter: Option<[u8; 16]>) -> AureaResult<Arc<ZenGpuContext>> {
    DEFAULT_GPU_CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow().upgrade()
            && !context.is_lost()
            && adapter.is_none_or(|uuid| context.adapter_uuid() == uuid)
        {
            return Ok(context);
        }
        let context = Arc::new(match adapter {
            Some(uuid) => ZenGpuContext::on_adapter(uuid)?,
            None => ZenGpuContext::new()?,
        });
        *slot.borrow_mut() = Arc::downgrade(&context);
        Ok(context)
    })
}

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
    let default_context = context.is_none();
    let context = match context {
        Some(context) => context,
        None => default_gpu_context(None)?,
    };
    let mut gpu =
        ZenGpuRenderer::with_context(&handles, Arc::clone(&context), width, height, scale_factor)?;
    if let Some(limits) = state.borrow().gpu_cache_limits {
        gpu.set_gpu_cache_limits(limits);
    }
    {
        let mut st = state.borrow_mut();
        st.gpu_context = Some(context);
        if default_context {
            st.gpu_context_default = true;
        }
    }
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

#[cfg(all(test, feature = "zengpu"))]
mod tests {
    use super::*;

    #[test]
    fn default_context_is_shared_without_a_permanent_owner() {
        let Ok(first) = default_gpu_context(None) else {
            return;
        };
        let second = default_gpu_context(None).expect("same device");
        assert!(Arc::ptr_eq(&first, &second));
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(second);
        assert!(
            weak.upgrade().is_none(),
            "the cache must not keep the GPU alive"
        );
    }

    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn shared_context_loss_reopens_once_on_the_same_adapter() {
        let Ok(lost) = default_gpu_context(None) else {
            return;
        };
        lost.device().simulate_loss();
        let first = default_gpu_context(Some(lost.adapter_uuid())).expect("replacement");
        let second = default_gpu_context(Some(lost.adapter_uuid())).expect("shared replacement");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &lost));
        assert!(!first.is_lost());
        assert_eq!(first.adapter_uuid(), lost.adapter_uuid());
    }
}
