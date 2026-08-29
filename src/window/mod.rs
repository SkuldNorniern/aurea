//! Window management, events, and lifecycle integration.

mod clipboard;
#[cfg(feature = "zengpu")]
mod gpu;
mod manager;
mod types;

pub use aurea_foundation::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};
pub use aurea_runtime::EventQueue;
pub use clipboard::{clipboard_text, set_clipboard_text};
pub use manager::WindowManager;
pub use types::{CursorGrabMode, WindowId, WindowType};

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
    dispatch_window_events, process_window_updates, register_event_callback, register_event_queue,
    register_update_callback, register_update_callbacks, unregister_event_callbacks,
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
    os::raw::c_void,
    rc::Rc,
    sync::{Arc, LazyLock, Mutex, OnceLock},
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
    pub handle: *mut c_void,
    pub menu_bar: Option<MenuBar>,
    pub content: Option<Box<dyn Element>>,
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
        register_update_callbacks(handle);

        // Register lifecycle bridge
        let eq_clone = event_queue.clone();
        let handle_usize = handle as usize;
        register_lifecycle_callback(
            handle,
            Arc::new(move |event| {
                let handle_ptr = handle_usize as *mut c_void;
                match event {
                    LifecycleEvent::WindowWillClose => {
                        eq_clone.push(WindowEvent::CloseRequested);
                    }
                    LifecycleEvent::WindowMoved => {
                        let mut x = 0;
                        let mut y = 0;
                        unsafe {
                            ng_platform_window_get_position(handle_ptr, &mut x, &mut y);
                        }
                        eq_clone.push(WindowEvent::Moved { x, y });
                    }
                    LifecycleEvent::WindowResized => {
                        let mut w = 0;
                        let mut h = 0;
                        unsafe {
                            ng_platform_window_get_size(handle_ptr, &mut w, &mut h);
                        }
                        eq_clone.push(WindowEvent::Resized {
                            width: w.cast_unsigned(),
                            height: h.cast_unsigned(),
                        });
                    }
                    LifecycleEvent::WindowMinimized => {
                        eq_clone.push(WindowEvent::Minimized);
                    }
                    LifecycleEvent::WindowRestored => {
                        eq_clone.push(WindowEvent::Restored);
                    }
                    LifecycleEvent::SurfaceLost => {
                        eq_clone.push(WindowEvent::SurfaceLost);
                    }
                    LifecycleEvent::SurfaceRecreated => {
                        eq_clone.push(WindowEvent::SurfaceRecreated);
                    }
                    _ => {}
                }
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

    pub fn create_menu_bar(&mut self) -> AureaResult<MenuBar> {
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

        Ok(MenuBar::new(handle))
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

    /// Takes this window's events without pumping the platform queue.
    ///
    /// The queue belongs to the process, not to any one window, so a caller
    /// holding several windows pumps once and drains each. Going through
    /// [`Self::poll_events`] for every window pumps once per window, which is
    /// wasted work and lets one window's turn dispatch another's messages.
    pub(super) fn drain_events(&self) -> Vec<WindowEvent> {
        proxy::drain_for(self);

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

    /// Check if the window is visible
    pub fn is_visible(&self) -> bool {
        unsafe { ng_platform_window_is_visible(self.handle) != 0 }
    }
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
