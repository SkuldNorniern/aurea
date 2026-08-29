//! A canvas that can be shared with a draw callback.
//!
//! [`Canvas`] is neither `Send` nor `Sync`, because the native canvas under it
//! belongs to the UI thread. But [`Canvas::set_draw_callback`] wants a
//! callback that is, and so does
//! [`FrameScheduler::register_ticker`](aurea_runtime::FrameScheduler::register_ticker),
//! because both are held in registries shared across threads. An application
//! that keeps one canvas and draws to it from its callback is therefore stuck:
//! the thing it needs to capture cannot be captured.
//!
//! Both applications built on Aurea reached the same workaround — a canvas
//! behind a mutex with `Send` and `Sync` asserted onto it — and wrote it out
//! almost identically, down to the same bug in each: forwarding
//! [`Element::handle`] without forwarding
//! [`Element::released_to_parent`], so a container taking the canvas left the
//! canvas underneath still believing it owned the native element.
//!
//! This is that wrapper, written once. The assertion it makes is the same one,
//! and it is only true under the same condition, stated plainly below.
//!
//! # What makes this sound
//!
//! The mutex gives exclusive access, which is not the same as thread affinity,
//! and a mutex alone would not make a UI object safe to touch from anywhere.
//! What makes it sound is the rule the caller keeps: **a `SharedCanvas` is
//! only ever locked on the UI thread.** The `Send` and `Sync` here exist so
//! the value can be *stored* in a callback the framework holds, not so it can
//! be *used* from another thread.
//!
//! To reach a canvas from a background thread, do not lock it there. Send the
//! work to the UI thread with [`WindowProxy::dispatch`](crate::WindowProxy),
//! or ask for a redraw with
//! [`request_canvas_redraw`](super::request_canvas_redraw), which takes a
//! [`CanvasId`](super::CanvasId) and is `Send` on its own account.

use std::ops::{Deref, DerefMut};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex, MutexGuard};

use aurea_foundation::lock;

use super::{Canvas, CanvasId};
use crate::elements::Element;
use crate::render::Rect;

/// A [`Canvas`] with `Send` and `Sync` asserted onto it.
///
/// Private: the assertion is only true under the rule in the module docs, and
/// [`SharedCanvas`] is what keeps callers to it.
struct SendableCanvas(Canvas);

// SAFETY: the canvas is only touched under the lock, on the UI thread. See the
// module documentation for why the lock alone is not the reason this holds.
unsafe impl Send for SendableCanvas {}
unsafe impl Sync for SendableCanvas {}

/// A canvas several places can hold: the window it is content of, and the
/// callback that draws it.
///
/// Cloning shares one canvas rather than copying it. Set it as a window's
/// content directly — it is an [`Element`], and hands ownership of the native
/// element over the way any other element does.
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
    ///
    /// Call this on the UI thread. Holding the guard across anything that
    /// pumps events would deadlock, so keep it short.
    ///
    /// The borrow is mutable through [`DerefMut`], because a canvas takes
    /// `&mut self` for drawing — and the whole point of sharing one is to
    /// draw on it.
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
    /// Another handle on the same canvas, not a second canvas.
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
        // Forwarded, not defaulted. The canvas underneath is what owns the
        // native element, and a wrapper that stayed quiet here would leave it
        // freeing an element its new parent had already taken.
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
