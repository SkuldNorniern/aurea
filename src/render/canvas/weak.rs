//! A canvas handle that does not keep the canvas alive, for its own callbacks.

use std::cell::RefCell;
use std::os::raw::c_void;
use std::rc::{Rc, Weak};

use aurea_render::{InteractionRegistry, Renderer, RendererBackend};

#[cfg(feature = "wgpu")]
use crate::platform::handles::NativeWindowHandle;

use super::Canvas;
use super::lifecycle::CanvasCleanup;
use super::state::CanvasState;

/// A [`Canvas`] that does not keep it alive, from [`Canvas::downgrade`].
#[derive(Clone)]
pub struct WeakCanvas {
    handle: *mut c_void,
    state: Weak<RefCell<CanvasState>>,
    renderer: Weak<RefCell<Option<Box<dyn Renderer>>>>,
    backend: RendererBackend,
    interaction_registry: Weak<InteractionRegistry>,
    #[cfg(feature = "wgpu")]
    surface_handle: Weak<NativeWindowHandle>,
    cleanup: Weak<CanvasCleanup>,
}

impl WeakCanvas {
    /// The canvas, while any `Canvas` for it is still around.
    pub fn upgrade(&self) -> Option<Canvas> {
        Some(Canvas {
            handle: self.handle,
            _cleanup: self.cleanup.upgrade()?,
            state: self.state.upgrade()?,
            renderer: self.renderer.upgrade()?,
            backend: self.backend,
            interaction_registry: self.interaction_registry.upgrade()?,
            #[cfg(feature = "wgpu")]
            surface_handle: self.surface_handle.upgrade()?,
        })
    }
}

impl Canvas {
    /// A handle that does not keep this canvas alive.
    pub fn downgrade(&self) -> WeakCanvas {
        WeakCanvas {
            handle: self.handle,
            state: Rc::downgrade(&self.state),
            renderer: Rc::downgrade(&self.renderer),
            backend: self.backend,
            interaction_registry: Rc::downgrade(&self.interaction_registry),
            #[cfg(feature = "wgpu")]
            surface_handle: Rc::downgrade(&self.surface_handle),
            cleanup: Rc::downgrade(&self._cleanup),
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn a_callback_holding_a_weak_canvas_lets_it_go() {
        let canvas = Canvas::new(32, 32, RendererBackend::Cpu).expect("canvas");
        let weak = canvas.downgrade();
        let inner = canvas.downgrade();
        canvas
            .set_draw_callback(move |_| {
                let _ = inner.upgrade().map(|c| c.width());
                Ok(())
            })
            .expect("callback");
        assert!(weak.upgrade().is_some());
        drop(canvas);
        assert!(weak.upgrade().is_none(), "nothing kept it alive");
    }
}
