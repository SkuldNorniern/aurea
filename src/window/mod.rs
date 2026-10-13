//! Window management, events, and lifecycle integration.

mod clipboard;
#[cfg(feature = "zengpu")]
mod gpu;
mod manager;
mod types;

pub use aurea_foundation::{
    EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons, PenPhase, PenSample, PenTool,
    WindowEvent,
};
pub use aurea_runtime::EventQueue;
pub use clipboard::{clipboard_text, set_clipboard_text};
pub use manager::WindowManager;
pub use types::{CursorGrabMode, CursorIcon, WindowId, WindowType};

use crate::dispatch;
use crate::elements::Element;
use crate::ffi::ng_platform_request_frame;
use crate::ffi::*;
#[cfg(feature = "wgpu")]
use crate::integration::NativeWindowHandle;
use crate::lifecycle::{
    LifecycleEvent, register_lifecycle_callback, subscribe_lifecycle_callback,
    unregister_lifecycle_callback,
};
use crate::menu::MenuBar;
mod proxy;

pub use proxy::WindowProxy;

#[cfg(feature = "wgpu")]
use crate::platform::handles::native_handle_from_window_ptr;
use crate::platform::ui_thread;
use crate::registry::window::{
    begin_event_capture, dispatch_window_events, end_event_capture, process_window_updates,
    register_event_callback, register_event_queue, register_update_callback,
    register_update_callbacks, scale_factor_changed, unregister_event_callbacks,
    unregister_event_queue, unregister_update_callbacks,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult};
use aurea_foundation::lock;
use aurea_foundation::{Capability, CapabilityChecker};
use aurea_foundation::{DesktopPlatform, Platform};
use aurea_runtime::{DamageRegion, FrameScheduler};
use std::{
    ffi::CString,
    os::raw::{c_int, c_void},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, LazyLock, Mutex, OnceLock},
    time::Duration,
};

/// Whether the platform is up, and how many windows are relying on it.
///
/// One lock rather than a flag and a counter: they describe a single thing,
/// and apart they could disagree. Bringing the platform up is several steps
/// and two threads reaching the first window at once must not each run them;
/// worse, a thread dropping the last window could tear the platform down
/// while another was midway through building one against it, because the
/// count only rose once the native window already existed. Claiming a window
/// and readying the platform now happen together, so the count is never
/// briefly zero while a window is being built.
struct PlatformState {
    /// Whether the native platform is currently initialised. Goes back to
    /// `false` when the last window is dropped, so the next window brings it
    /// up again — platform init is not once per process.
    ready: bool,
    /// Live windows, plus any part-built one. `ng_platform_cleanup()` runs
    /// only at zero: calling it while other windows are alive would destroy
    /// shared state (`UnregisterClassA` on Windows) out from under them.
    windows: usize,
}

static PLATFORM: LazyLock<Mutex<PlatformState>> = LazyLock::new(|| {
    Mutex::new(PlatformState {
        ready: false,
        windows: 0,
    })
});

/// Readies the platform if it is down, and counts one window against it.
///
/// The caller owns that count until it calls [`release_platform`].
fn acquire_platform() -> AureaResult<()> {
    let mut state = lock(&PLATFORM);
    if !state.ready {
        if unsafe { ng_platform_init() } != 0 {
            return Err(AureaError::PlatformError(1));
        }
        FrameScheduler::set_request_frame_hook(|| {
            unsafe { ng_platform_request_frame() };
        });
        // Whichever thread brings the platform up owns the native UI. On
        // Apple targets that has to be the process main thread, and claiming
        // it from elsewhere is refused rather than recorded — the platform is
        // up at this point, so it is torn back down before returning.
        if let Err(error) = ui_thread::claim() {
            unsafe { ng_platform_cleanup() };
            state.ready = false;
            return Err(error);
        }
        state.ready = true;
    }
    state.windows += 1;
    Ok(())
}

/// Gives back one window's claim, tearing the platform down at the last.
fn release_platform() {
    let mut state = lock(&PLATFORM);
    state.windows = state.windows.saturating_sub(1);
    if state.windows == 0 && state.ready {
        unsafe { ng_platform_cleanup() };
        state.ready = false;
        ui_thread::release();
    }
}

pub use crate::registry::window::{
    process_all_window_events, process_all_window_updates, push_window_event,
};

use log::{error, info};

/// The platform layer's success code, from `common/errors.h`.
const NG_SUCCESS: i32 = 0;
/// The platform layer's "no implementation for this" code, which is a
/// statement about the backend rather than a failure.
const NG_ERROR_UNSUPPORTED: i32 = -5;

/// A native platform window with menu, content, and event handling.
pub struct Window {
    /// The native window. Read through [`Self::handle`], which is the
    /// deliberate escape hatch; writing it would leave everything registered
    /// against the old one.
    handle: *mut c_void,
    /// This window's menu bar, freed with the window.
    menu_bar: Option<MenuBar>,
    /// What the window shows, freed with the window.
    content: Option<Box<dyn Element>>,
    platform: Platform,
    capabilities: CapabilityChecker,
    damage: Mutex<DamageRegion>,
    scale_factor: Mutex<f32>,
    event_queue: Arc<EventQueue>,
    /// Addresses this window for the life of the process, so a proxy outliving
    /// its window cannot be pointed at whichever window reuses its handle.
    /// This window's identity, allocated once and never reused.
    id: WindowId,
    /// The window's native handle, kept so a wgpu surface can borrow something
    /// that lives as long as the window does. `NativeWindowHandle` is an inert
    /// identifier and stays `Send + Sync`; the `Window` around it is not.
    #[cfg(feature = "wgpu")]
    surface_handle: Arc<NativeWindowHandle>,
    window_type: WindowType,
    /// Set by [`Window::close`], so the close it asks for is not reported
    /// again as a close request.
    closing: Arc<AtomicBool>,
}

impl Window {
    /// Create a new window with default type (Normal).
    pub fn new(title: &str, width: i32, height: i32) -> AureaResult<Self> {
        Self::with_type(title, width, height, WindowType::Normal)
    }

    /// Create a new window with the specified type.
    pub fn with_type(
        title: &str,
        width: i32,
        height: i32,
        window_type: WindowType,
    ) -> AureaResult<Self> {
        const AUREA_FFI_ABI_VERSION: i32 = 2;

        // The ABI check really is once per process: the native library cannot
        // change under a running program. The outcome is stored rather than
        // just the fact that it ran, because a `Once` whose closure failed
        // still counts as completed, and a later call would sail past it.
        static ABI_CHECK: OnceLock<Result<(), AureaError>> = OnceLock::new();

        ABI_CHECK
            .get_or_init(|| {
                let got = unsafe { ng_platform_get_abi_version() };
                if got != AUREA_FFI_ABI_VERSION {
                    return Err(AureaError::AbiVersionMismatch {
                        expected: AUREA_FFI_ABI_VERSION,
                        got,
                    });
                }
                Ok(())
            })
            .clone()?;

        // The claim is taken before anything native is created, and the guard
        // below gives it back if construction does not finish.
        acquire_platform()?;
        let mut guard = WindowBuildGuard {
            handle: None,
            id: None,
            armed: true,
        };

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();

        info!("Creating window: {}x{}", width, height);

        // A sheet is a macOS document-modal panel with no counterpart
        // elsewhere. Saying so beats handing back an ordinary window that
        // does not behave like one.
        if window_type == WindowType::Sheet && platform != Platform::Desktop(DesktopPlatform::MacOS)
        {
            return Err(AureaError::Unsupported {
                operation: "create a sheet window",
                platform,
            });
        }

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let window_type_int = match window_type {
            WindowType::Normal => 0,
            WindowType::Popup => 1,
            WindowType::Tool => 2,
            WindowType::Utility => 3,
            WindowType::Sheet => 4,
            WindowType::Dialog => 5,
        };
        let handle = unsafe {
            ng_platform_create_window_with_type(title.as_ptr(), width, height, window_type_int)
        };

        if handle.is_null() {
            return Err(AureaError::WindowCreationFailed);
        }
        guard.handle = Some(handle);

        let scale_factor = unsafe { ng_platform_get_scale_factor(handle) };
        let event_queue = Arc::new(EventQueue::new());

        register_event_queue(handle, &event_queue);
        scale_factor_changed(handle, scale_factor);
        register_update_callbacks(handle);

        // Register lifecycle bridge
        let closing = Arc::new(AtomicBool::new(false));
        let eq_clone = event_queue.clone();
        let closing_bridge = Arc::clone(&closing);
        let handle_usize = handle as usize;
        register_lifecycle_callback(
            handle,
            Arc::new(move |event| {
                let handle_ptr = handle_usize as *mut c_void;
                let event = match event {
                    LifecycleEvent::WindowWillClose => {
                        if take_forced_close(&closing_bridge) {
                            WindowEvent::Closed
                        } else {
                            WindowEvent::CloseRequested
                        }
                    }
                    LifecycleEvent::WindowMoved => {
                        let mut x = 0;
                        let mut y = 0;
                        unsafe {
                            ng_platform_window_get_position(handle_ptr, &mut x, &mut y);
                        }
                        WindowEvent::Moved { x, y }
                    }
                    LifecycleEvent::WindowResized => {
                        let mut w = 0;
                        let mut h = 0;
                        unsafe {
                            ng_platform_window_get_size(handle_ptr, &mut w, &mut h);
                        }
                        WindowEvent::Resized {
                            width: w.cast_unsigned(),
                            height: h.cast_unsigned(),
                        }
                    }
                    LifecycleEvent::WindowMinimized => WindowEvent::Minimized,
                    LifecycleEvent::WindowRestored => WindowEvent::Restored,
                    LifecycleEvent::SurfaceLost => WindowEvent::SurfaceLost,
                    LifecycleEvent::SurfaceRecreated => WindowEvent::SurfaceRecreated,
                    _ => return,
                };
                eq_clone.push(event);
                // Queued here rather than through push_window_event, so the
                // wake is ours to ask for. An idle loop runs no frame by
                // itself, and the event would wait for an unrelated one.
                FrameScheduler::wake();
            }),
        );

        unsafe {
            ng_platform_window_set_lifecycle_callback(handle);
            ng_platform_window_set_scale_factor_callback(handle, ng_invoke_scale_factor_changed);
        }

        let id = WindowId::claim(handle);
        proxy::register(id);
        guard.id = Some(id);

        #[cfg(feature = "wgpu")]
        let surface_handle = Arc::new(
            native_handle_from_window_ptr(handle).ok_or(AureaError::WindowCreationFailed)?,
        );

        guard.disarm();
        Ok(Self {
            handle,
            id,
            #[cfg(feature = "wgpu")]
            surface_handle,
            menu_bar: None,
            content: None,
            platform,
            capabilities,
            damage: Mutex::new(DamageRegion::new(16)),
            scale_factor: Mutex::new(scale_factor),
            event_queue,
            closing,
            window_type,
        })
    }

    /// Set the window position
    pub fn set_position(&self, x: i32, y: i32) {
        ui_thread::check("Window::set_position");
        unsafe {
            ng_platform_window_set_position(self.handle, x, y);
        }
    }

    /// Get the window position
    pub fn position(&self) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
        unsafe {
            ng_platform_window_get_position(self.handle, &mut x, &mut y);
        }
        (x, y)
    }

    /// Builds this window's menu bar and hands back a borrow of it.
    ///
    /// The window keeps it. A menu is attached to a window and the platform
    /// frees the two together, so handing back an owned `MenuBar` gave it a
    /// life of its own: dropping it early destroyed the menu of a window that
    /// was still showing it.
    ///
    /// Calling this twice replaces the previous menu bar.
    pub fn create_menu_bar(&mut self) -> AureaResult<&mut MenuBar> {
        if !self.capabilities.has(Capability::MenuBar) {
            return Err(AureaError::ElementOperationFailed);
        }

        let handle = unsafe { ng_platform_create_menu() };
        if handle.is_null() {
            return Err(AureaError::MenuCreationFailed);
        }

        let result = unsafe { ng_platform_attach_menu(self.handle, handle) };
        if result != 0 {
            unsafe { ng_platform_destroy_menu(handle) };
            return Err(AureaError::MenuCreationFailed);
        }

        // Replacing any previous one frees it here, which is what attaching a
        // second menu to the window did to the first anyway.
        Ok(self.menu_bar.insert(MenuBar::new(handle)))
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn capabilities(&self) -> &CapabilityChecker {
        &self.capabilities
    }

    /// Get the window type
    pub fn window_type(&self) -> WindowType {
        self.window_type
    }

    /// Get the stable window identifier for this window.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// A `Send + Sync` handle for reaching this window from another thread.
    ///
    /// See [`WindowProxy`] for how queued work gets back onto the UI thread.
    pub fn proxy(&self) -> WindowProxy {
        WindowProxy::new(self.id)
    }

    /// This window's process-unique id, which proxies address it by.
    pub(super) fn proxy_id(&self) -> WindowId {
        self.id
    }

    pub fn run(&self) -> AureaResult<()> {
        ui_thread::check("Window::run");
        let result = unsafe { ng_platform_run() };
        if result != 0 {
            return Err(AureaError::EventLoopError);
        }
        Ok(())
    }

    /// Register a per-window update callback.
    ///
    /// The callback is called once per `process_frames()` invocation.
    pub fn on_update<F>(&self, callback: F)
    where
        F: Fn(WindowId) + Send + Sync + 'static,
    {
        register_update_callback(self.handle, callback);
    }

    /// Sets the window's content, replacing whatever was there.
    ///
    /// The window takes ownership of the element: it is destroyed with the
    /// window, or when it is replaced.
    ///
    /// Replacing destroys the previous content. It cannot be dropped to
    /// achieve that — it gave up ownership when it became the content — so its
    /// native element is destroyed explicitly here. The platform does not free
    /// the old content when new content is set, so without this it would be
    /// left parented and invisible for the life of the window.
    pub fn set_content<E>(&mut self, element: E) -> AureaResult<()>
    where
        E: Element + 'static,
    {
        ui_thread::check("Window::set_content");
        let content_handle = element.handle();
        if content_handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let result = unsafe { ng_platform_set_window_content(self.handle, content_handle) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        // The window frees its content, so the element stops freeing itself.
        element.released_to_parent();

        if let Some(previous) = self.content.replace(Box::new(element)) {
            let stale = previous.handle();
            drop(previous);
            if !stale.is_null() && stale != content_handle {
                // Detach first, then destroy. The old content gave up
                // ownership when it became the content, so something has to
                // take it back before it can be freed — on AppKit that is a
                // retain, and destroying without it would be an over-release.
                //
                // Which is why the result matters. Destroying after a failed
                // detach is the over-release this order exists to avoid, so a
                // failure leaves the element alone: leaked, but leaked
                // safely, and said out loud.
                let detached = unsafe { ng_platform_detach_element(stale) };
                if detached == NG_SUCCESS || detached == NG_ERROR_UNSUPPORTED {
                    // Unsupported means the backend has no separate detach
                    // step, not that anything went wrong — freeing the
                    // element is all there ever was to do there.
                    unsafe { ng_platform_destroy_element(stale) };
                } else {
                    error!(
                        "could not reclaim the previous window content                          (error {detached}); leaving it rather than freeing                          something this window no longer owns"
                    );
                }
            }
        }
        Ok(())
    }

    pub fn schedule_frame(&self) {
        FrameScheduler::schedule();
    }

    pub fn add_damage(&self, rect: Rect) {
        let mut damage = lock(&self.damage);
        damage.add(rect);
        self.schedule_frame();
    }

    pub fn take_damage(&self) -> Option<Rect> {
        let mut damage = lock(&self.damage);
        damage.take()
    }

    pub fn scale_factor(&self) -> f32 {
        *lock(&self.scale_factor)
    }

    pub fn update_scale_factor(&self) {
        let new_scale = unsafe { ng_platform_get_scale_factor(self.handle) };
        *lock(&self.scale_factor) = new_scale;
    }

    pub fn on_lifecycle_event<F>(&self, callback: F)
    where
        F: Fn(LifecycleEvent) + Send + Sync + 'static,
    {
        let window_handle = self.handle;
        // Added alongside Aurea's own bridge rather than over it: replacing
        // it stopped this window delivering CloseRequested, Resized and Moved.
        subscribe_lifecycle_callback(window_handle, Arc::new(callback));

        unsafe {
            ng_platform_window_set_lifecycle_callback(window_handle);
        }
    }

    /// Get the native window handle for external renderer integration
    ///
    /// This returns a platform-specific window handle that can be used to create
    /// surfaces for external rendering APIs (e.g., wgpu).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// let native_handle = window.native_handle();
    /// // Use native_handle with external rendering APIs
    /// # Ok(())
    /// # }
    /// ```
    /// `None` when the platform will not give one up, rather than a handle
    /// that only looks like one.
    #[cfg(feature = "wgpu")]
    pub fn native_handle(&self) -> Option<NativeWindowHandle> {
        use crate::integration::wgpu::WindowNativeHandle;
        WindowNativeHandle::native_handle_impl(self)
    }

    /// The stored handle a wgpu surface target borrows from.
    #[cfg(feature = "wgpu")]
    pub(crate) fn surface_target(&self) -> &NativeWindowHandle {
        &self.surface_handle
    }

    /// Poll window events (non-blocking)
    ///
    /// This method processes all pending window events by calling registered callbacks
    /// and returns the events for manual processing. It should be called from an
    /// external event loop to process window events.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut window = Window::new("App", 800, 600)?;
    ///
    /// // Register callbacks
    /// window.on_event(|event| {
    ///     match event {
    ///         aurea::WindowEvent::CloseRequested => {
    ///             println!("Window close requested");
    ///         }
    ///         _ => {}
    ///     }
    /// });
    ///
    /// // In your event loop:
    /// loop {
    ///     let events = window.poll_events(); // Callbacks are called automatically
    ///     // You can also manually process events if needed
    ///     for event in events {
    ///         match event {
    ///             aurea::WindowEvent::CloseRequested => break,
    ///             _ => {}
    ///         }
    ///     }
    ///     window.process_frames()?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn poll_events(&self) -> Vec<WindowEvent> {
        ui_thread::check("Window::poll_events");
        pump_platform_events();
        self.drain_events()
    }

    /// Waits until something happens, then hands back this window's events,
    /// for an app that runs its own loop instead of [`Self::run`].
    ///
    /// Returns once input comes in, a frame is asked for from any thread (a
    /// canvas redraw, a ticker, a [`WindowProxy`] dispatch) or `timeout`
    /// passes; `None` waits with no limit. Frames that fall due run inside the
    /// wait, paced to the display the way [`Self::run`] paces them. A loop
    /// built on this needs neither [`Self::process_frames`] nor a sleep: it
    /// takes no CPU while nothing happens and adds no delay when something
    /// does.
    ///
    /// The events have already reached the [`Self::on_event`] callbacks, as
    /// with [`Self::poll_events`]. Android and iOS run their own loop and only
    /// hand back what is queued.
    ///
    /// ```rust,no_run
    /// # fn main() -> aurea::AureaResult<()> {
    /// let window = aurea::Window::new("App", 800, 600)?;
    /// window.show();
    /// loop {
    ///     for event in window.wait_events(None) {
    ///         if matches!(event, aurea::WindowEvent::CloseRequested) {
    ///             return Ok(());
    ///         }
    ///     }
    /// }
    /// # }
    /// ```
    pub fn wait_events(&self, timeout: Option<Duration>) -> Vec<WindowEvent> {
        ui_thread::check("Window::wait_events");
        let timeout_ms =
            timeout.map_or(-1, |t| c_int::try_from(t.as_millis()).unwrap_or(c_int::MAX));
        // A frame inside the wait dispatches the queue to the callbacks, so
        // what it dispatches for this window is kept for the caller too.
        begin_event_capture(self.handle, Vec::new());
        unsafe { ng_platform_wait_events(timeout_ms) };
        let mut events = end_event_capture();
        events.extend(self.drain_events());
        events
    }

    /// Takes this window's events without pumping the platform queue.
    ///
    /// The queue belongs to the process, not to any one window, so a caller
    /// holding several windows pumps once and drains each. Going through
    /// [`Self::poll_events`] for every window pumps once per window, which is
    /// wasted work and lets one window's turn dispatch another's messages.
    pub(super) fn drain_events(&self) -> Vec<WindowEvent> {
        proxy::drain_for(self);
        dispatch::drain();

        // Drain the queue, then hand the events to this window's callbacks and
        // return them for manual processing too.
        let events = self.event_queue.pop_all();
        for event in &events {
            if let WindowEvent::ScaleFactorChanged { scale_factor } = event {
                *lock(&self.scale_factor) = *scale_factor;
            }
        }
        dispatch_window_events(self.handle, &events);
        events
    }

    /// Process scheduled frames (event-driven canvas redraws)
    ///
    /// This method processes all scheduled frames by calling redraw callbacks
    /// on registered canvases. It should be called from an external event loop
    /// after processing window events.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut window = Window::new("App", 800, 600)?;
    ///
    /// // In your event loop:
    /// loop {
    ///     let events = window.poll_events();
    ///     // Process events...
    ///     window.process_frames()?; // Process scheduled canvas redraws
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn process_frames(&self) -> AureaResult<()> {
        ui_thread::check("Window::process_frames");
        proxy::drain_for(self);
        dispatch::drain();
        process_window_updates(self.handle);
        FrameScheduler::process_frames()
    }

    /// Register an event callback (retained-mode style)
    ///
    /// This registers a callback that will be called for all window events.
    /// The callback is retained for the lifetime of the window.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    ///
    /// window.on_event(|event| {
    ///     match event {
    ///         aurea::WindowEvent::CloseRequested => {
    ///             println!("Window close requested");
    ///         }
    ///         aurea::WindowEvent::Resized { width, height } => {
    ///             println!("Window resized to {}x{}", width, height);
    ///         }
    ///         _ => {}
    ///     }
    /// });
    ///
    /// // Call poll_events() in your event loop to trigger callbacks
    /// # Ok(())
    /// # }
    /// ```
    pub fn on_event<F>(&self, callback: F)
    where
        F: Fn(WindowEvent) + 'static,
    {
        ui_thread::check("Window::on_event");
        register_event_callback(self.handle, Rc::new(callback));
    }

    /// Request the window to close
    ///
    /// This sends a close request to the window. The window may emit a
    /// `CloseRequested` event that can be handled by event callbacks.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// window.request_close();
    /// # Ok(())
    /// # }
    /// ```
    pub fn request_close(&self) {
        unsafe {
            ng_platform_window_request_close(self.handle);
        }
    }

    /// Set the window title
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// window.set_title("New Title");
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_title(&self, title: &str) -> AureaResult<()> {
        ui_thread::check("Window::set_title");
        let title_cstr = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        unsafe {
            ng_platform_window_set_title(self.handle, title_cstr.as_ptr());
        }
        Ok(())
    }

    /// Set the native application/window icon from tightly packed RGBA8 pixels.
    pub fn set_icon_rgba(&self, rgba: &[u8], width: u32, height: u32) -> AureaResult<()> {
        ui_thread::check("Window::set_icon_rgba");
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(AureaError::ElementOperationFailed)?;
        if width == 0
            || height == 0
            || width > i32::MAX as u32
            || height > i32::MAX as u32
            || rgba.len() != expected
        {
            return Err(AureaError::ElementOperationFailed);
        }
        let result =
            unsafe { ng_platform_window_set_icon_rgba(self.handle, rgba.as_ptr(), width, height) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Set the window size
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// window.set_size(1024, 768);
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_size(&self, width: u32, height: u32) {
        ui_thread::check("Window::set_size");
        unsafe {
            ng_platform_window_set_size(self.handle, width as i32, height as i32);
        }
    }

    /// Get the window size
    ///
    /// Returns `(width, height)` in pixels.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// let (width, height) = window.size();
    /// println!("Window size: {}x{}", width, height);
    /// # Ok(())
    /// # }
    /// ```
    pub fn size(&self) -> (u32, u32) {
        let mut width = 0i32;
        let mut height = 0i32;
        unsafe {
            ng_platform_window_get_size(self.handle, &mut width, &mut height);
        }
        (width.cast_unsigned(), height.cast_unsigned())
    }

    /// Check if the window is currently focused
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use aurea::Window;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let window = Window::new("App", 800, 600)?;
    /// if window.is_focused() {
    ///     println!("Window is focused");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_focused(&self) -> bool {
        unsafe { ng_platform_window_is_focused(self.handle) != 0 }
    }

    /// Set cursor visibility for this window
    pub fn set_cursor_visible(&self, visible: bool) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor_visible");
        let result =
            unsafe { ng_platform_window_set_cursor_visible(self.handle, i32::from(visible)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Sets the cursor shown over this window's content, until changed.
    ///
    /// Cheap enough to call every frame with whatever is under the pointer.
    pub fn set_cursor(&self, icon: CursorIcon) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor");
        let result = unsafe { ng_platform_window_set_cursor_icon(self.handle, icon as i32) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Tells the input method where the text caret is, in logical pixels
    /// relative to the content, so its candidate window opens beside the
    /// text instead of in a corner. Call it when a text field gains focus
    /// and whenever its caret moves; composed text then arrives as
    /// [`WindowEvent::ImePreedit`] and committed text as
    /// [`WindowEvent::TextInput`].
    pub fn set_ime_cursor_area(&self, area: Rect) -> AureaResult<()> {
        ui_thread::check("Window::set_ime_cursor_area");
        let result = unsafe {
            ng_platform_window_set_ime_cursor_area(
                self.handle,
                area.x,
                area.y,
                area.width,
                area.height,
            )
        };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Keys this window keeps for itself even while a child view, such as
    /// an embedded web view, has keyboard focus. They arrive as
    /// [`WindowEvent::KeyInput`] on this window and do not reach the child.
    /// Replaces any set before; an empty slice clears them.
    ///
    /// Covers child views in this process. A view whose input runs in
    /// another process (WebView2) has to forward its keys itself.
    pub fn set_shortcuts(&self, shortcuts: &[(KeyCode, Modifiers)]) -> AureaResult<()> {
        ui_thread::check("Window::set_shortcuts");
        let (keycodes, modifiers): (Vec<u32>, Vec<u32>) = shortcuts
            .iter()
            .filter_map(|(key, mods)| Some((key.to_raw()?, mods.bits())))
            .unzip();
        let count =
            i32::try_from(keycodes.len()).map_err(|_| AureaError::ElementOperationFailed)?;
        let result = unsafe {
            ng_platform_window_set_shortcuts(
                self.handle,
                keycodes.as_ptr(),
                modifiers.as_ptr(),
                count,
            )
        };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Set cursor grab mode for this window
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor_grab");
        let result = unsafe { ng_platform_window_set_cursor_grab(self.handle, mode as i32) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Get the native window handle
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    /// Show the window
    pub fn show(&self) {
        ui_thread::check("Window::show");
        unsafe {
            ng_platform_window_show(self.handle);
        }
    }

    /// Hide the window (without destroying it)
    pub fn hide(&self) {
        ui_thread::check("Window::hide");
        unsafe {
            ng_platform_window_hide(self.handle);
        }
    }

    /// Makes closing the window hide it instead of destroying it.
    ///
    /// `CloseRequested` still arrives, but the window, its content and the
    /// event loop stay alive, so an app can keep running from a
    /// [`TrayIcon`](crate::TrayIcon) and [`show`](Self::show) the window
    /// again. To quit, turn this off and [`request_close`](Self::request_close).
    ///
    /// Windows and macOS have this; elsewhere it reports
    /// [`AureaError::Unsupported`].
    pub fn set_hide_on_close(&self, hide: bool) -> AureaResult<()> {
        ui_thread::check("Window::set_hide_on_close");
        if !cfg!(any(windows, target_os = "macos")) {
            return Err(AureaError::Unsupported {
                operation: "hide a window on close",
                platform: Platform::current(),
            });
        }
        let result = unsafe { ng_platform_window_set_hide_on_close(self.handle, i32::from(hide)) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// Keeps the window open when the person asks it to close.
    ///
    /// While guarded, the close box, Alt+F4, Cmd+W and the window manager's
    /// close only deliver [`WindowEvent::CloseRequested`]. The window, its
    /// content and its state are left as they were. Unguarded, which is the
    /// default, closing works as it always has.
    ///
    /// The pattern is to guard the window, ask on `CloseRequested`, then
    /// either call [`close`](Self::close) or do nothing:
    ///
    /// ```text
    /// window.set_close_guard(true);
    /// // on WindowEvent::CloseRequested:
    /// //   MessageDialog::new("Save changes?")
    /// //       .buttons(MessageButtons::SaveDiscardCancel)
    /// //       .show(&window)
    /// //   Save    => save, then window.close()
    /// //   Discard => window.close()
    /// //   Cancel  => nothing; the window stays open
    /// ```
    pub fn set_close_guard(&self, guarded: bool) {
        ui_thread::check("Window::set_close_guard");
        unsafe {
            ng_platform_window_set_close_guard(self.handle, i32::from(guarded));
        }
    }

    /// Closes the window, even while it is guarded.
    ///
    /// This is how an app acts on a `CloseRequested` it held back. It clears
    /// the guard and asks for the close the same way as
    /// [`request_close`](Self::request_close). The `CloseRequested` that this
    /// close produces arrives as [`WindowEvent::Closed`] rather than a second
    /// `CloseRequested`, so the app is not asked again, and a loop pumping
    /// the window by hand stops on `Closed`. Hide-on-close still applies, so
    /// turn it off first if the window should really be destroyed.
    ///
    /// See [`set_close_guard`](Self::set_close_guard) for the pattern.
    pub fn close(&self) {
        ui_thread::check("Window::close");
        self.set_close_guard(false);
        self.closing.store(true, Ordering::Release);
        self.request_close();
    }

    /// Check if the window is visible
    pub fn is_visible(&self) -> bool {
        unsafe { ng_platform_window_is_visible(self.handle) != 0 }
    }
}

/// Whether a window-will-close notice is the one `close` asked for. It is
/// consumed, so only the first such notice becomes `Closed`.
fn take_forced_close(closing: &AtomicBool) -> bool {
    closing.swap(false, Ordering::AcqRel)
}

/// Runs the platform's message queue.
///
/// On Windows this drives PeekMessage/DispatchMessage, which runs WndProc —
/// filling event queues and delivering WM_PAINT for any invalidated canvas.
/// Without it a manual poll loop never repaints and never receives input.
pub(super) fn pump_platform_events() {
    unsafe { ng_platform_poll_events() };
}

/// Undoes a half-built window.
///
/// Creating a window registers callbacks, opens a proxy queue and counts
/// towards the platform staying alive. If a later step fails, none of that is
/// anyone's responsibility yet: the `Window` that would have freed it in its
/// own `Drop` was never returned.
struct WindowBuildGuard {
    /// Set once the native window exists; before that there is none to free.
    handle: Option<*mut c_void>,
    /// Set once a proxy queue is open for it.
    id: Option<WindowId>,
    armed: bool,
}

impl WindowBuildGuard {
    /// The window is built and owns its registrations now.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for WindowBuildGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        match (self.handle, self.id) {
            (Some(handle), Some(id)) => teardown_window(handle, id),
            // Nothing native was registered yet, so only the platform claim
            // taken at the top of construction has to go back.
            _ => {
                if let Some(handle) = self.handle {
                    unsafe { ng_platform_destroy_window(handle) };
                }
                release_platform();
            }
        }
    }
}

/// Releases everything `Window::new` registered for `handle`.
fn teardown_window(handle: *mut c_void, id: WindowId) {
    unregister_lifecycle_callback(handle);
    unregister_event_queue(handle);
    unregister_update_callbacks(handle);
    unregister_event_callbacks(handle);
    proxy::clear_for(id);
    WindowId::forget(handle);

    unsafe {
        ng_platform_destroy_window(handle);
    }

    release_platform();
}

impl Drop for Window {
    fn drop(&mut self) {
        teardown_window(self.handle, self.id);
    }
}

// `Window` is deliberately neither `Send` nor `Sync`. Native window systems
// are thread-affine — AppKit and UIKit demand the main thread, GTK the thread
// that called `gtk_init`, Win32 the thread that created the window — so a
// window that could travel between threads would be a standing invitation to
// undefined behaviour. Background threads reach the UI through
// [`WindowProxy`] instead.
//
// The raw pointer field is what makes it so; there is no impl to write.

#[cfg(test)]
mod event_queue_tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn event_queue_push_pop_all() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        queue.push(WindowEvent::Focused);
        let out = queue.pop_all();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], WindowEvent::CloseRequested));
        assert!(matches!(out[1], WindowEvent::Focused));
        assert!(queue.pop_all().is_empty());
    }

    /// A close that `close` asked for is reported as such once; later
    /// closes are requests again.
    #[test]
    fn forced_close_is_swallowed_once() {
        let closing = AtomicBool::new(true);
        assert!(take_forced_close(&closing));
        assert!(!take_forced_close(&closing));
    }

    /// Handlers are keyed by window handle in a thread-local registry, and may
    /// capture values that are neither `Send` nor `Sync`.
    #[test]
    fn dispatch_invokes_registered_callbacks() {
        let handle = 0xBEEF as *mut c_void;
        let received = Rc::new(RefCell::new(Vec::new()));
        let rec = Rc::clone(&received);
        register_event_callback(handle, Rc::new(move |e| rec.borrow_mut().push(e)));

        dispatch_window_events(handle, &[WindowEvent::CloseRequested, WindowEvent::Focused]);

        assert_eq!(received.borrow().len(), 2);
        unregister_event_callbacks(handle);
    }

    #[test]
    fn dispatch_after_unregister_is_a_noop() {
        let handle = 0xF00D as *mut c_void;
        let seen = Rc::new(RefCell::new(0));
        let seen_clone = Rc::clone(&seen);
        register_event_callback(handle, Rc::new(move |_| *seen_clone.borrow_mut() += 1));
        unregister_event_callbacks(handle);

        dispatch_window_events(handle, &[WindowEvent::Focused]);

        assert_eq!(*seen.borrow(), 0);
    }

    #[test]
    fn modifiers_from_bits_and_is_any() {
        let none = Modifiers::from_bits(0);
        assert!(!none.is_any());
        assert!(!none.shift && !none.ctrl && !none.alt && !none.meta);

        let shift = Modifiers::from_bits(0b0001);
        assert!(shift.is_any());
        assert!(shift.shift && !shift.ctrl);

        let all = Modifiers::from_bits(0b1111);
        assert!(all.is_any());
        assert!(all.shift && all.ctrl && all.alt && all.meta);
    }

    #[test]
    fn modifiers_default() {
        let m = Modifiers::default();
        assert!(!m.is_any());
    }

    #[test]
    fn event_queue_key_input() {
        let queue = EventQueue::new();
        let mods = Modifiers::from_bits(0b0010);
        queue.push(WindowEvent::KeyInput {
            key: KeyCode::A,
            pressed: true,
            modifiers: mods,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        assert!(matches!(
            &out[0],
            WindowEvent::KeyInput {
                key: KeyCode::A,
                pressed: true,
                ..
            }
        ));
    }

    #[test]
    fn event_queue_mouse_button() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            modifiers: Modifiers::default(),
            x: 10.0,
            y: 20.0,
            click_count: 1,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::MouseButton {
                button,
                pressed,
                x,
                y,
                click_count,
                ..
            } => {
                assert_eq!(*button, MouseButton::Left);
                assert!(*pressed);
                assert_eq!((*x, *y), (10.0, 20.0));
                assert_eq!(*click_count, 1);
            }
            _ => unreachable!("just pushed a MouseButton event"),
        }
    }

    #[test]
    fn event_queue_mouse_wheel() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::MouseWheel {
            delta_x: 1.0,
            delta_y: -2.0,
            modifiers: Modifiers::default(),
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::MouseWheel {
                delta_x, delta_y, ..
            } => {
                assert_eq!(*delta_x, 1.0);
                assert_eq!(*delta_y, -2.0);
            }
            _ => unreachable!("just pushed a MouseWheel event"),
        }
    }

    #[test]
    fn event_queue_text_input() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::TextInput {
            text: "hello".into(),
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::TextInput { text } => assert_eq!(text, "hello"),
            _ => unreachable!("just pushed a TextInput event"),
        }
    }

    #[test]
    fn event_queue_focus() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::Focused);
        queue.push(WindowEvent::Unfocused);
        let out = queue.pop_all();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], WindowEvent::Focused));
        assert!(matches!(out[1], WindowEvent::Unfocused));
    }

    #[test]
    fn event_queue_resized() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::Resized {
            width: 800,
            height: 600,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        assert!(matches!(
            &out[0],
            WindowEvent::Resized {
                width: 800,
                height: 600
            }
        ));
    }

    #[test]
    fn event_queue_scale_factor_changed() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::ScaleFactorChanged { scale_factor } => assert_eq!(*scale_factor, 2.0),
            _ => unreachable!("just pushed a ScaleFactorChanged event"),
        }
    }
}

#[cfg(test)]
mod window_content_tests {
    use crate::{AureaResult, Window};

    /// Replacing a window's content destroys the old content. The platform does
    /// not free it, and the old wrapper cannot: it gave up ownership when it
    /// became the content.
    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn replacing_window_content_destroys_the_old_content() -> AureaResult<()> {
        use crate::elements::{Element, Label};

        let mut window = Window::new("replace", 300, 200)?;

        let first = Label::new("first")?;
        let first_handle = first.handle();
        window.set_content(first)?;

        let second = Label::new("second")?;
        let second_handle = second.handle();
        window.set_content(second)?;

        assert_ne!(first_handle, second_handle);
        // The old label is gone and the window still works.
        window.set_title("replaced")?;
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod gtk_multi_window_tests {
    //! Closing one window must not take the whole application with it.
    //!
    //! `ng_linux_run` enters `gtk_main`, and the window `destroy` handler used to
    //! call `gtk_main_quit` unconditionally — so on GTK, closing any one window of
    //! several ended the loop and every remaining window went with it. The
    //! `poll_events` tests cannot catch that: they drive the main context directly
    //! and never enter `gtk_main`, which is the only state in which the bug shows.
    //!
    //! This needs a display and runs a real main loop, so it is `#[ignore]`d. Run
    //! it with:
    //!
    //! ```text
    //! cargo test --lib window::gtk_multi_window_tests -- --ignored
    //! ```

    use crate::{AureaResult, Window};
    use std::mem::forget;
    use std::os::raw::{c_int, c_void};
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

    unsafe extern "C" {
        fn gtk_widget_destroy(widget: *mut c_void);
        fn gtk_main_level() -> c_uint;
        fn g_idle_add(function: extern "C" fn(*mut c_void) -> c_int, data: *mut c_void) -> c_uint;
        fn g_timeout_add(
            interval: c_uint,
            function: extern "C" fn(*mut c_void) -> c_int,
            data: *mut c_void,
        ) -> c_uint;
    }
    #[allow(non_camel_case_types)]
    type c_uint = u32;

    const REMOVE: c_int = 0;

    /// The loop depth seen after the first window closed. Zero means the loop had
    /// already been quit, which is the bug.
    static LEVEL_AFTER_FIRST_CLOSE: AtomicI32 = AtomicI32::new(-1);
    /// The second window, closed once the check has run so the loop can end.
    static SECOND: AtomicI32 = AtomicI32::new(0);
    static SECOND_PTR: AtomicUsize = AtomicUsize::new(0);
    static FIRST_PTR: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn close_first(_: *mut c_void) -> c_int {
        unsafe { gtk_widget_destroy(FIRST_PTR.load(Ordering::SeqCst) as *mut c_void) };
        REMOVE
    }

    /// Runs after the first window is gone. If the loop is still turning we get
    /// here at all, and the depth it reports is the evidence.
    extern "C" fn observe_then_close_second(_: *mut c_void) -> c_int {
        LEVEL_AFTER_FIRST_CLOSE.store(unsafe { gtk_main_level() } as i32, Ordering::SeqCst);
        unsafe { gtk_widget_destroy(SECOND_PTR.load(Ordering::SeqCst) as *mut c_void) };
        REMOVE
    }

    /// Stops a hung loop so a failure is a failed assert rather than a hang.
    extern "C" fn bail_out(_: *mut c_void) -> c_int {
        unsafe {
            if gtk_main_level() > 0 {
                gtk_main_quit();
            }
        }
        REMOVE
    }

    unsafe extern "C" {
        fn gtk_main_quit();
    }

    #[test]
    #[ignore = "runs a real GTK main loop; run with --ignored"]
    fn closing_one_window_leaves_the_others_running() -> AureaResult<()> {
        let first = Window::new("first", 200, 150)?;
        let second = Window::new("second", 200, 150)?;
        first.show();
        second.show();

        FIRST_PTR.store(first.handle() as usize, Ordering::SeqCst);
        SECOND_PTR.store(second.handle() as usize, Ordering::SeqCst);
        SECOND.store(1, Ordering::SeqCst);

        unsafe {
            g_idle_add(close_first, null_mut());
            g_timeout_add(150, observe_then_close_second, null_mut());
            g_timeout_add(3000, bail_out, null_mut());
        }

        // Returns when the last window goes, or when bail_out fires.
        first.run()?;

        // Both widgets are already gone, destroyed above. Dropping the Rust values
        // would destroy them a second time, which GTK reports and ignores; leaking
        // them keeps that noise out of the test output.
        forget(first);
        forget(second);

        assert!(
            LEVEL_AFTER_FIRST_CLOSE.load(Ordering::SeqCst) > 0,
            "the loop ended when the first of two windows closed, so the second \
             window's application would have exited with it"
        );
        Ok(())
    }
}
#[cfg(all(test, target_os = "linux"))]
mod gtk_shortcuts_tests {
    //! Window shortcuts reach the window while a child widget has focus, on GTK,
    //! and typing still reaches a focused native entry.
    //!
    //! Key events are built and dispatched with `gtk_main_do_event`, the route a
    //! real key takes from GDK: toplevel handlers first, then the focus widget.
    //!
    //! ```text
    //! cargo test --lib window::gtk_shortcuts_tests -- --ignored --test-threads=1
    //! ```

    use crate::elements::{Container, Element, Orientation, Stack, TextField};
    use crate::{AureaResult, KeyCode, Modifiers, Window, WindowEvent};
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::thread::sleep;
    use std::time::Duration;

    const GDK_KEY_PRESS: i32 = 8;
    const GDK_KEY_F6: u32 = 0xffc3;
    const GDK_KEY_B: u32 = 0x062;

    /// `GdkEventKey` from GTK 3.
    #[repr(C)]
    struct GdkEventKey {
        kind: i32,
        window: *mut c_void,
        send_event: i8,
        time: u32,
        state: u32,
        keyval: u32,
        length: i32,
        string: *mut c_char,
        hardware_keycode: u16,
        group: u8,
        is_modifier: u32,
    }

    unsafe extern "C" {
        fn gtk_widget_get_window(widget: *mut c_void) -> *mut c_void;
        fn gtk_window_set_focus(window: *mut c_void, focus: *mut c_void);
        fn gtk_entry_get_text(entry: *mut c_void) -> *const c_char;
        fn gdk_event_new(kind: i32) -> *mut GdkEventKey;
        fn gdk_event_free(event: *mut GdkEventKey);
        fn gtk_main_do_event(event: *mut GdkEventKey);
        fn g_object_ref(object: *mut c_void) -> *mut c_void;
        fn gdk_keyval_to_unicode(keyval: u32) -> u32;
    }

    fn window_with_field() -> AureaResult<(Window, *mut c_void)> {
        let mut window = Window::new("shortcuts", 300, 200)?;
        let field = TextField::new()?;
        let entry = field.handle();
        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(field)?;
        window.set_content(stack)?;
        window.show();
        // A key needs the window's GdkWindow, which exists once it is realized.
        for _ in 0..100 {
            let _ = window.poll_events();
            if !unsafe { gtk_widget_get_window(window.handle()) }.is_null() {
                break;
            }
            sleep(Duration::from_millis(5));
        }
        unsafe { gtk_window_set_focus(window.handle(), entry) };
        Ok((window, entry))
    }

    /// Presses `keyval` on the window the way GDK delivers a real key.
    fn press(window: &Window, keyval: u32) {
        unsafe {
            let event = gdk_event_new(GDK_KEY_PRESS);
            let gdk_window = gtk_widget_get_window(window.handle());
            (*event).window = g_object_ref(gdk_window);
            (*event).send_event = 1;
            (*event).keyval = keyval;
            let text = char::from_u32(gdk_keyval_to_unicode(keyval)).filter(|c| !c.is_control());
            (*event).length = i32::from(text.is_some());
            gtk_main_do_event(event);
            gdk_event_free(event);
        }
    }

    fn pressed(window: &Window) -> Vec<KeyCode> {
        window
            .poll_events()
            .into_iter()
            .filter_map(|event| match event {
                WindowEvent::KeyInput {
                    key, pressed: true, ..
                } => Some(key),
                _ => None,
            })
            .collect()
    }

    fn entry_text(entry: *mut c_void) -> String {
        unsafe { CStr::from_ptr(gtk_entry_get_text(entry)) }
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_shortcut_reaches_the_window_and_not_the_child() -> AureaResult<()> {
        let (window, entry) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::B, Modifiers::default())])?;

        press(&window, GDK_KEY_B);
        assert!(pressed(&window).contains(&KeyCode::B));
        assert_eq!(entry_text(entry), "", "the entry must not see the shortcut");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn typing_still_reaches_a_focused_entry() -> AureaResult<()> {
        let (window, entry) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::F5, Modifiers::default())])?;

        press(&window, GDK_KEY_B);
        let _ = pressed(&window);
        assert_eq!(entry_text(entry), "b");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn keys_that_are_not_shortcuts_are_still_reported() -> AureaResult<()> {
        let (window, _entry) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::F5, Modifiers::default())])?;

        press(&window, GDK_KEY_F6);
        assert!(pressed(&window).contains(&KeyCode::F6));
        Ok(())
    }
}
#[cfg(all(test, target_os = "linux"))]
mod gtk_window_drop_tests {
    //! Dropping a window the platform has already destroyed must be harmless.
    //!
    //! Closing a window from the window manager destroys the widget, but the Rust
    //! value that owns it lives until it is dropped, and then asks for the same
    //! window to be destroyed again. A GTK widget is a pointer rather than a
    //! handle, so the second destroy reads memory that has been freed unless the
    //! platform checks first.
    //!
    //! This lives in its own binary because the UI thread is claimed per process,
    //! and a second test in the same binary would run on another thread.
    //!
    //! ```text
    //! cargo test --lib window::gtk_window_drop_tests -- --ignored
    //! ```

    use crate::{AureaResult, Window};
    use std::os::raw::c_void;

    unsafe extern "C" {
        fn gtk_widget_destroy(widget: *mut c_void);
        fn g_log_set_always_fatal(mask: u32) -> u32;
    }

    /// GLib reports a destroy of something that is not a widget and carries on, so
    /// the bug this covers is a message on stderr rather than a failure. Making
    /// that message fatal is what turns it into one.
    const G_LOG_LEVEL_CRITICAL: u32 = 1 << 3;

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn dropping_a_window_the_platform_already_closed_is_harmless() -> AureaResult<()> {
        unsafe { g_log_set_always_fatal(G_LOG_LEVEL_CRITICAL) };

        let window = Window::new("already closed", 200, 150)?;
        window.show();

        // What the window manager does when the user clicks the close button.
        unsafe { gtk_widget_destroy(window.handle()) };

        // The owner still believes it has a window to free.
        drop(window);
        Ok(())
    }
}
#[cfg(all(test, windows))]
mod win32_ime_tests {
    //! Input method messages reach the application as preedit and text events.
    //!
    //! A real composition needs an installed IME, so these drive the window
    //! with the messages an IME sends and check what comes out.
    //!
    //! ```text
    //! cargo test --lib window::win32_ime_tests -- --ignored --test-threads=1
    //! ```

    use crate::render::Rect;
    use crate::{AureaResult, Window, WindowEvent};
    use std::os::raw::c_void;

    const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
    const WM_IME_COMPOSITION: u32 = 0x010F;
    const GCS_RESULTSTR: isize = 0x0800;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
    }

    fn send(window: &Window, msg: u32, l: isize) {
        unsafe { SendMessageW(window.handle(), msg, 0, l) };
    }

    fn opened() -> AureaResult<Window> {
        let window = Window::new("ime", 300, 200)?;
        window.show();
        let _ = window.poll_events();
        Ok(window)
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn ending_a_composition_clears_the_preedit() -> AureaResult<()> {
        let window = opened()?;
        send(&window, WM_IME_ENDCOMPOSITION, 0);

        let preedits: Vec<_> = window
            .poll_events()
            .into_iter()
            .filter_map(|event| match event {
                WindowEvent::ImePreedit { text, cursor } => Some((text, cursor)),
                _ => None,
            })
            .collect();
        assert_eq!(preedits, [(String::new(), None)]);
        Ok(())
    }

    /// The window handles the composition message itself, so an empty result
    /// must not turn into typed text through `WM_CHAR` either.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn an_empty_result_types_nothing() -> AureaResult<()> {
        let window = opened()?;
        send(&window, WM_IME_COMPOSITION, GCS_RESULTSTR);

        let typed = window
            .poll_events()
            .into_iter()
            .any(|event| matches!(event, WindowEvent::TextInput { .. }));
        assert!(!typed);
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn the_caret_area_can_be_set() -> AureaResult<()> {
        let window = opened()?;
        window.set_ime_cursor_area(Rect::new(40.0, 20.0, 2.0, 18.0))
    }
}
#[cfg(all(test, windows))]
mod win32_multi_window_tests {
    //! Closing one window must not take the whole application with it.
    //!
    //! The window procedure drops a window from tracking on `WM_DESTROY` and posts
    //! the quit message only once nothing is left. Getting that wrong ends the
    //! application the moment a popup or tool window closes, which is invisible to
    //! every test that uses a single window.
    //!
    //! `DestroyWindow` dispatches `WM_DESTROY` synchronously, so the decision has
    //! already been made by the time it returns and the queue can be inspected
    //! directly — no event loop, and no waiting.
    //!
    //! These create real native windows, so they are `#[ignore]`d. Run them with:
    //!
    //! ```text
    //! cargo test --lib window::win32_multi_window_tests -- --ignored --test-threads=1
    //! ```

    use crate::{AureaError, AureaResult, Window};
    use std::mem::forget;
    use std::os::raw::c_void;
    use std::ptr::null_mut;

    const WM_QUIT: u32 = 0x0012;
    const PM_REMOVE: u32 = 0x0001;

    #[repr(C)]
    #[derive(Default)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt_x: i32,
        pt_y: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn DestroyWindow(hwnd: *mut c_void) -> i32;
        fn PeekMessageA(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, remove: u32) -> i32;
    }

    /// Whether the quit message reaches the queue.
    ///
    /// The quit request is a flag rather than a queued message, and it only
    /// surfaces once nothing else is waiting — so a filtered peek walks straight
    /// past it while a destroyed window's messages are still pending. Draining
    /// the queue is what actually answers the question.
    fn quit_was_posted() -> bool {
        let mut msg = Msg {
            hwnd: null_mut(),
            ..Default::default()
        };
        let mut found = false;
        unsafe {
            while PeekMessageA(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    found = true;
                }
            }
        }
        found
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn closing_one_window_leaves_the_others_running() -> AureaResult<()> {
        let first = Window::new("first", 200, 150)?;
        let second = Window::new("second", 200, 150)?;
        first.show();
        second.show();

        assert!(!quit_was_posted(), "nothing has closed yet");

        unsafe { DestroyWindow(first.handle()) };
        assert!(
            !quit_was_posted(),
            "closing the first of two windows ended the loop, so the second \
             window's application would have exited with it"
        );

        unsafe { DestroyWindow(second.handle()) };
        assert!(
            quit_was_posted(),
            "closing the last window should end the loop"
        );

        // Both windows are already destroyed. Dropping the Rust values would call
        // DestroyWindow again on stale handles.
        forget(first);
        forget(second);
        Ok(())
    }

    /// Every window kind used to come out as a plain overlapped window, so asking
    /// for a popup or a tool palette silently got something else.
    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn each_window_kind_gets_its_own_style() -> AureaResult<()> {
        use crate::WindowType;

        const GWL_STYLE: i32 = -16;
        const GWL_EXSTYLE: i32 = -20;
        const WS_POPUP: u32 = 0x8000_0000;
        const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
        const WS_EX_DLGMODALFRAME: u32 = 0x0000_0001;

        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowLongA(hwnd: *mut c_void, index: i32) -> i32;
        }
        let style =
            |w: &Window, index| unsafe { GetWindowLongA(w.handle(), index).cast_unsigned() };

        let normal = Window::new("normal", 200, 150)?;
        let popup = Window::with_type("popup", 200, 150, WindowType::Popup)?;
        let tool = Window::with_type("tool", 200, 150, WindowType::Tool)?;
        let dialog = Window::with_type("dialog", 200, 150, WindowType::Dialog)?;

        assert_eq!(
            style(&normal, GWL_STYLE) & WS_POPUP,
            0,
            "normal is not a popup"
        );
        assert_ne!(
            style(&popup, GWL_STYLE) & WS_POPUP,
            0,
            "a popup is borderless"
        );
        assert_ne!(
            style(&tool, GWL_EXSTYLE) & WS_EX_TOOLWINDOW,
            0,
            "a tool window stays out of the taskbar"
        );
        assert_ne!(
            style(&dialog, GWL_EXSTYLE) & WS_EX_DLGMODALFRAME,
            0,
            "a dialog has a dialog frame"
        );

        // A sheet is macOS-only, and says so rather than handing back a window
        // that does not behave like one.
        assert!(matches!(
            Window::with_type("sheet", 200, 150, WindowType::Sheet),
            Err(AureaError::Unsupported { .. })
        ));
        Ok(())
    }
}
#[cfg(all(test, windows))]
mod win32_shortcuts_tests {
    //! Window shortcuts reach the window while a child control has focus.
    //!
    //! ```text
    //! cargo test --lib window::win32_shortcuts_tests -- --ignored --test-threads=1
    //! ```

    use crate::elements::{Container, Element, Orientation, Stack, TextField};
    use crate::{AureaResult, KeyCode, Modifiers, Window, WindowEvent};
    use std::os::raw::c_void;

    const WM_KEYDOWN: u32 = 0x0100;
    const VK_F5: usize = 0x74;
    const VK_F6: usize = 0x75;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn PostMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> i32;
    }

    /// A window whose only content is a native text field, and that field.
    fn window_with_field() -> AureaResult<(Window, *mut c_void)> {
        let mut window = Window::new("shortcuts", 300, 200)?;
        let field = TextField::new()?;
        let child = field.handle();
        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(field)?;
        window.set_content(stack)?;
        window.show();
        let _ = window.poll_events();
        Ok((window, child))
    }

    fn key_down_to(child: *mut c_void, vk: usize) {
        unsafe { PostMessageW(child, WM_KEYDOWN, vk, 0) };
    }

    fn pressed(window: &Window) -> Vec<KeyCode> {
        window
            .poll_events()
            .into_iter()
            .filter_map(|event| match event {
                WindowEvent::KeyInput {
                    key, pressed: true, ..
                } => Some(key),
                _ => None,
            })
            .collect()
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_shortcut_typed_into_a_child_reaches_the_window() -> AureaResult<()> {
        let (window, child) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::F5, Modifiers::default())])?;

        key_down_to(child, VK_F5);
        assert_eq!(pressed(&window), [KeyCode::F5]);
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn other_keys_stay_with_the_child() -> AureaResult<()> {
        let (window, child) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::F5, Modifiers::default())])?;

        key_down_to(child, VK_F6);
        assert!(!pressed(&window).contains(&KeyCode::F6));
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn clearing_shortcuts_gives_keys_back() -> AureaResult<()> {
        let (window, child) = window_with_field()?;
        window.set_shortcuts(&[(KeyCode::F5, Modifiers::default())])?;
        window.set_shortcuts(&[])?;

        key_down_to(child, VK_F5);
        assert!(!pressed(&window).contains(&KeyCode::F5));
        Ok(())
    }
}
#[cfg(all(test, windows))]
mod win32_text_input_tests {
    //! Text typed in any script has to arrive intact.
    //!
    //! The window class used to be registered with `RegisterClassExA`. For an
    //! ANSI class Windows delivers `WM_CHAR` carrying a byte in the thread's
    //! codepage rather than a UTF-16 code unit, while the text-input path read it
    //! as UTF-16 either way — so anything outside that codepage's Latin range
    //! arrived as mojibake, and an application wanting Korean, Japanese or
    //! Cyrillic input had to guess a codepage and repair the text itself.
    //!
    //! ```text
    //! cargo test --lib window::win32_text_input_tests -- --ignored --test-threads=1
    //! ```

    use crate::{AureaResult, Window, WindowEvent};
    use std::os::raw::c_void;

    const WM_CHAR: u32 = 0x0102;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
    }

    /// Types one UTF-16 code unit at the window, the way the keyboard would.
    fn type_unit(window: &Window, unit: u16) {
        unsafe { SendMessageW(window.handle(), WM_CHAR, unit as usize, 0) };
    }

    fn typed_text(window: &Window) -> String {
        window
            .poll_events()
            .into_iter()
            .filter_map(|event| match event {
                WindowEvent::TextInput { text } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn text_outside_the_ansi_codepage_arrives_intact() -> AureaResult<()> {
        let window = Window::new("text", 300, 200)?;
        window.show();
        let _ = window.poll_events();

        // U+D55C HANGUL SYLLABLE HAN, then U+0416 CYRILLIC ZHE: both outside a
        // Latin ANSI codepage, and neither representable in the other's.
        type_unit(&window, 0xD55C);
        type_unit(&window, 0x0416);

        assert_eq!(typed_text(&window), "한Ж");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn plain_ascii_still_arrives() -> AureaResult<()> {
        let window = Window::new("text", 300, 200)?;
        window.show();
        let _ = window.poll_events();

        for unit in "Hi!".encode_utf16() {
            type_unit(&window, unit);
        }

        assert_eq!(typed_text(&window), "Hi!");
        Ok(())
    }

    /// Characters outside the BMP arrive as a surrogate pair, two messages.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_surrogate_pair_arrives_as_one_character() -> AureaResult<()> {
        let window = Window::new("text", 300, 200)?;
        window.show();
        let _ = window.poll_events();

        // U+1F600 GRINNING FACE.
        for unit in "\u{1F600}".encode_utf16() {
            type_unit(&window, unit);
        }

        assert_eq!(typed_text(&window), "\u{1F600}");
        Ok(())
    }

    /// A title in any script has to survive the trip. It crosses the FFI as UTF-8
    /// and used to be handed to the ANSI entry point, which read it as the
    /// thread's codepage.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_window_title_keeps_its_script() -> AureaResult<()> {
        const WM_GETTEXT: u32 = 0x000D;

        let window = Window::new("한글 제목", 300, 200)?;
        window.set_title("Ω 제목 テスト")?;

        let mut buf = [0u16; 64];
        let len = unsafe {
            SendMessageW(
                window.handle(),
                WM_GETTEXT,
                buf.len(),
                buf.as_mut_ptr() as isize,
            )
        };
        let len = usize::try_from(len).unwrap_or(0);

        assert_eq!(String::from_utf16_lossy(&buf[..len]), "Ω 제목 テスト");
        Ok(())
    }
}
