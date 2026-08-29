//! A canvas that can be shared with a draw callback.

use std::ops::{Deref, DerefMut};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex, MutexGuard};

use aurea_foundation::lock;

use super::{Canvas, CanvasId};
use crate::elements::Element;
use crate::render::Rect;

struct SendableCanvas(Canvas);

// SAFETY: the canvas is only touched under the lock, on the UI thread. See the
unsafe impl Send for SendableCanvas {}
unsafe impl Sync for SendableCanvas {}

/// A canvas several places can hold: the window it is content of, and the callback that draws it.
///
/// ```rust,no_run
/// use aurea::render::{Canvas, RendererBackend, SharedCanvas};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let canvas = SharedCanvas::new(Canvas::new(800, 600, RendererBackend::Cpu)?);
///
/// // The callback keeps its own handle on the same canvas.
/// let for_callback = canvas.clone();
/// canvas.get().set_draw_callback(move |ctx| {
///     let _ = &for_callback;
///     ctx.clear(aurea::render::Color::rgb(20, 20, 24))
/// })?;
/// # Ok(())
/// # }
/// ```
pub struct SharedCanvas {
    inner: Arc<Mutex<SendableCanvas>>,
}

impl SharedCanvas {
    /// Shares a canvas.
    pub fn new(canvas: Canvas) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SendableCanvas(canvas))),
        }
    }

    /// Borrows the canvas.
    pub fn get(&self) -> SharedCanvasRef<'_> {
        SharedCanvasRef {
            guard: lock(&self.inner),
        }
    }

    /// The canvas's identity, which is `Send` and can be held anywhere.
    pub fn id(&self) -> CanvasId {
        self.get().id()
    }
}

impl Clone for SharedCanvas {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Element for SharedCanvas {
    fn handle(&self) -> *mut c_void {
        self.get().handle()
    }

    fn released_to_parent(&self) {
        self.get().released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        let canvas = self.get();
        unsafe { Element::invalidate_platform(&*canvas, rect) }
    }
}

/// A borrowed [`SharedCanvas`], which derefs to the [`Canvas`].
pub struct SharedCanvasRef<'a> {
    guard: MutexGuard<'a, SendableCanvas>,
}

impl Deref for SharedCanvasRef<'_> {
    type Target = Canvas;

    fn deref(&self) -> &Canvas {
        &self.guard.0
    }
}

impl DerefMut for SharedCanvasRef<'_> {
    fn deref_mut(&mut self) -> &mut Canvas {
        &mut self.guard.0
    }
}
