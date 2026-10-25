//! Window management, events, and lifecycle integration.

pub mod clipboard;
#[cfg(feature = "zengpu")]
mod gpu;
mod manager;
mod types;

pub use aurea_foundation::{
    EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons, Occlusion, PenPhase, PenSample,
    PenTapAction, PenTool, WindowEvent,
};
pub use aurea_runtime::EventQueue;
pub use clipboard::{clipboard_text, set_clipboard_text};
pub use manager::WindowManager;
pub use types::{CursorGrabMode, CursorIcon, WindowId, WindowType};

use crate::dispatch;
use crate::elements::Element;
use crate::ffi::c_title;
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
use crate::render::{Image, Rect};
use crate::{AureaError, AureaResult};
use aurea_foundation::lock;
use aurea_foundation::{Capability, CapabilityChecker};
use aurea_foundation::{DesktopPlatform, Platform};
use aurea_runtime::{DamageRegion, FrameScheduler};
use std::{
    os::raw::{c_int, c_void},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, LazyLock, Mutex, OnceLock},
    time::Duration,
};

struct PlatformState {
    ready: bool,
    windows: usize,
}

static PLATFORM: LazyLock<Mutex<PlatformState>> = LazyLock::new(|| {
    Mutex::new(PlatformState {
        ready: false,
        windows: 0,
    })
});

fn acquire_platform() -> AureaResult<()> {
    let mut state = lock(&PLATFORM);
    if !state.ready {
        if unsafe { ng_platform_init() } != 0 {
            return Err(AureaError::PlatformError(1));
        }
        FrameScheduler::set_request_frame_hook(|| {
            unsafe { ng_platform_request_frame() };
        });
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

/// A native platform window with menu, content, and event handling.
pub struct Window {
    handle: *mut c_void,
    menu_bar: Option<MenuBar>,
    content: Option<Box<dyn Element>>,
    platform: Platform,
    capabilities: CapabilityChecker,
    damage: Mutex<DamageRegion>,
    scale_factor: Mutex<f32>,
    event_queue: Arc<EventQueue>,
    id: WindowId,
    #[cfg(feature = "wgpu")]
    surface_handle: Arc<NativeWindowHandle>,
    window_type: WindowType,
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

        acquire_platform()?;
        let mut guard = WindowBuildGuard {
            handle: None,
            id: None,
            armed: true,
        };

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();

        info!("Creating window: {}x{}", width, height);

        if window_type == WindowType::Sheet && platform != Platform::Desktop(DesktopPlatform::MacOS)
        {
            return Err(AureaError::Unsupported {
                operation: "create a sheet window",
                platform,
            });
        }

        let title = c_title(title)?;
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
                    LifecycleEvent::MemoryWarning => WindowEvent::MemoryWarning,
                    LifecycleEvent::Occlusion(level) => WindowEvent::OcclusionChanged(level),
                    LifecycleEvent::SurfaceLost => WindowEvent::SurfaceLost,
                    LifecycleEvent::SurfaceRecreated => WindowEvent::SurfaceRecreated,
                    _ => return,
                };
                eq_clone.push(event);
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

    /// Set the window position.
    pub fn set_position(&self, x: i32, y: i32) {
        ui_thread::check("Window::set_position");
        unsafe {
            ng_platform_window_set_position(self.handle, x, y);
        }
    }

    /// Get the window position.
    pub fn position(&self) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
        unsafe {
            ng_platform_window_get_position(self.handle, &mut x, &mut y);
        }
        (x, y)
    }

    /// Builds this window's menu bar and hands back a borrow of it.
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

        Ok(self.menu_bar.insert(MenuBar::new(handle)))
    }

    /// The platform this window runs on.
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the capability checker for this window.
    pub fn capabilities(&self) -> &CapabilityChecker {
        &self.capabilities
    }

    /// Get the window type.
    pub fn window_type(&self) -> WindowType {
        self.window_type
    }

    /// Get the stable window identifier for this window.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// A `Send + Sync` handle for reaching this window from another thread.
    pub fn proxy(&self) -> WindowProxy {
        WindowProxy::new(self.id)
    }

    pub(super) fn proxy_id(&self) -> WindowId {
        self.id
    }

    /// Runs the platform event loop.
    pub fn run(&self) -> AureaResult<()> {
        ui_thread::check("Window::run");
        let result = unsafe { ng_platform_run() };
        if result != 0 {
            return Err(AureaError::EventLoopError);
        }
        Ok(())
    }

    /// Register a per-window update callback.
    pub fn on_update<F>(&self, callback: F)
    where
        F: Fn(WindowId) + Send + Sync + 'static,
    {
        register_update_callback(self.handle, callback);
    }

    /// Sets the window's content, replacing whatever was there.
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

        element.released_to_parent();

        if let Some(previous) = self.content.replace(Box::new(element)) {
            let stale = previous.handle();
            drop(previous);
            if !stale.is_null() && stale != content_handle {
                let detached = unsafe { ng_platform_detach_element(stale) };
                if detached == NG_SUCCESS || detached == NG_ERROR_UNSUPPORTED {
                    unsafe { ng_platform_destroy_element(stale) };
                } else {
                    error!(
                        "could not reclaim the previous window content (error {detached}); leaving it rather than freeing something this window no longer owns"
                    );
                }
            }
        }
        Ok(())
    }

    /// Schedules a frame to be drawn.
    pub fn schedule_frame(&self) {
        FrameScheduler::schedule();
    }

    /// Adds `rect` to the damage that needs redrawing, and schedules a frame.
    pub fn add_damage(&self, rect: Rect) {
        let mut damage = lock(&self.damage);
        damage.add(rect);
        self.schedule_frame();
    }

    /// Takes the accumulated damage, if any.
    pub fn take_damage(&self) -> Option<Rect> {
        let mut damage = lock(&self.damage);
        damage.take()
    }

    /// Returns the window's current scale factor.
    pub fn scale_factor(&self) -> f32 {
        *lock(&self.scale_factor)
    }

    /// Reads the scale factor from the native window.
    pub fn update_scale_factor(&self) {
        let new_scale = unsafe { ng_platform_get_scale_factor(self.handle) };
        *lock(&self.scale_factor) = new_scale;
    }

    /// Registers a callback for window lifecycle events.
    pub fn on_lifecycle_event<F>(&self, callback: F)
    where
        F: Fn(LifecycleEvent) + Send + Sync + 'static,
    {
        let window_handle = self.handle;
        subscribe_lifecycle_callback(window_handle, Arc::new(callback));

        unsafe {
            ng_platform_window_set_lifecycle_callback(window_handle);
        }
    }

    /// Get the native window handle for external renderer integration.
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
    #[cfg(feature = "wgpu")]
    pub fn native_handle(&self) -> Option<NativeWindowHandle> {
        use crate::integration::wgpu::WindowNativeHandle;
        WindowNativeHandle::native_handle_impl(self)
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn surface_target(&self) -> &NativeWindowHandle {
        &self.surface_handle
    }

    /// Poll window events (non-blocking).
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

    /// Waits until something happens, then hands back this window's events, for an app that runs
    /// its own loop instead of [`Self::run`]. A [`WindowProxy`] from another thread ends the wait.
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
        begin_event_capture(self.handle, Vec::new());
        unsafe { ng_platform_wait_events(timeout_ms) };
        let mut events = end_event_capture();
        events.extend(self.drain_events());
        events
    }

    pub(super) fn drain_events(&self) -> Vec<WindowEvent> {
        proxy::drain_for(self);
        dispatch::drain();

        let events = self.event_queue.pop_all();
        for event in &events {
            if let WindowEvent::ScaleFactorChanged { scale_factor } = event {
                *lock(&self.scale_factor) = *scale_factor;
            }
        }
        dispatch_window_events(self.handle, &events);
        events
    }

    /// Process scheduled frames (event-driven canvas redraws).
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

    /// Register an event callback (retained-mode style).
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

    /// Request the window to close.
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

    /// Set the window title.
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
        let title_cstr = c_title(title)?;
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

    /// Set the window size.
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

    /// Sets the smallest content size, in logical pixels, the user can resize the window to; 0
    /// leaves that axis free. A window already smaller grows to it.
    pub fn set_min_size(&self, width: u32, height: u32) -> AureaResult<()> {
        ui_thread::check("Window::set_min_size");
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return Err(AureaError::ElementOperationFailed);
        };
        match unsafe { ng_platform_window_set_min_size(self.handle, width, height) } {
            0 => Ok(()),
            NG_ERROR_UNSUPPORTED => Err(AureaError::Unsupported {
                operation: "Window::set_min_size",
                platform: Platform::current(),
            }),
            _ => Err(AureaError::ElementOperationFailed),
        }
    }

    /// Get the window size.
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

    /// Check if the window is currently focused.
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

    /// How much of this window shows: other windows over it, minimized, off screen or on another
    /// virtual desktop. Windows measures the covered part; macOS tells only visible or covered,
    /// and Linux only shown or minimized. [`WindowEvent::OcclusionChanged`] says when it changes.
    pub fn occlusion(&self) -> Occlusion {
        Occlusion::from_raw(unsafe { ng_platform_window_occlusion(self.handle) })
    }

    /// Set cursor visibility for this window.
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
    pub fn set_cursor(&self, icon: CursorIcon) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor");
        let result = unsafe { ng_platform_window_set_cursor_icon(self.handle, icon as i32) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Sets an RGBA8 cursor with a hotspot in image pixels.
    pub fn set_cursor_image(&self, image: &Image, hotspot: (u32, u32)) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor_image");
        if image.width == 0
            || image.height == 0
            || hotspot.0 >= image.width
            || hotspot.1 >= image.height
            || (image.width as usize)
                .checked_mul(image.height as usize)
                .and_then(|n| n.checked_mul(4))
                != Some(image.data.len())
        {
            return Err(AureaError::ElementOperationFailed);
        }
        let result = unsafe {
            ng_platform_window_set_cursor_rgba(
                self.handle,
                image.data.as_ptr(),
                image.width,
                image.height,
                hotspot.0,
                hotspot.1,
            )
        };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Tells the input method where the text caret is, in logical pixels relative to the content,
    /// so its candidate window opens beside the text instead of in a corner.
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

    /// Keys this window keeps for itself even while a child view, such as an embedded web view, has
    /// keyboard focus.
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

    /// Set cursor grab mode for this window.
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> AureaResult<()> {
        ui_thread::check("Window::set_cursor_grab");
        let result = unsafe { ng_platform_window_set_cursor_grab(self.handle, mode as i32) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Get the native window handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    /// Show the window.
    pub fn show(&self) {
        ui_thread::check("Window::show");
        unsafe {
            ng_platform_window_show(self.handle);
        }
    }

    /// Hide the window (without destroying it).
    pub fn hide(&self) {
        ui_thread::check("Window::hide");
        unsafe {
            ng_platform_window_hide(self.handle);
        }
    }

    /// Makes closing the window hide it instead of destroying it.
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
    pub fn set_close_guard(&self, guarded: bool) {
        ui_thread::check("Window::set_close_guard");
        unsafe {
            ng_platform_window_set_close_guard(self.handle, i32::from(guarded));
        }
    }

    /// Closes the window, even while it is guarded.
    pub fn close(&self) {
        ui_thread::check("Window::close");
        self.set_close_guard(false);
        self.closing.store(true, Ordering::Release);
        self.request_close();
    }

    /// Check if the window is visible.
    pub fn is_visible(&self) -> bool {
        unsafe { ng_platform_window_is_visible(self.handle) != 0 }
    }
}

fn take_forced_close(closing: &AtomicBool) -> bool {
    closing.swap(false, Ordering::AcqRel)
}

pub(super) fn pump_platform_events() {
    unsafe { ng_platform_poll_events() };
}

struct WindowBuildGuard {
    handle: Option<*mut c_void>,
    id: Option<WindowId>,
    armed: bool,
}

impl WindowBuildGuard {
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
            _ => {
                if let Some(handle) = self.handle {
                    unsafe { ng_platform_destroy_window(handle) };
                }
                release_platform();
            }
        }
    }
}

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

    #[test]
    fn forced_close_is_swallowed_once() {
        let closing = AtomicBool::new(true);
        assert!(take_forced_close(&closing));
        assert!(!take_forced_close(&closing));
    }

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
            time: Default::default(),
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
        window.set_title("replaced")?;
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod gtk_multi_window_tests {
    //! Closing one window must not take the whole application with it.

    use crate::{AureaResult, Window};
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

    static LEVEL_AFTER_FIRST_CLOSE: AtomicI32 = AtomicI32::new(-1);
    static SECOND: AtomicI32 = AtomicI32::new(0);
    static SECOND_PTR: AtomicUsize = AtomicUsize::new(0);
    static FIRST_PTR: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn close_first(_: *mut c_void) -> c_int {
        unsafe { gtk_widget_destroy(FIRST_PTR.load(Ordering::SeqCst) as *mut c_void) };
        REMOVE
    }

    extern "C" fn observe_then_close_second(_: *mut c_void) -> c_int {
        LEVEL_AFTER_FIRST_CLOSE.store(unsafe { gtk_main_level() } as i32, Ordering::SeqCst);
        unsafe { gtk_widget_destroy(SECOND_PTR.load(Ordering::SeqCst) as *mut c_void) };
        REMOVE
    }

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

        first.run()?;

        drop(first);
        drop(second);
        assert_eq!(super::lock(&super::PLATFORM).windows, 0);

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
    //! Window shortcuts reach the window while a child widget has focus, on GTK, and typing still
    //! reaches a focused native entry.

    use crate::elements::{Container, Element, Orientation, Stack, TextField};
    use crate::{AureaResult, KeyCode, Modifiers, Window, WindowEvent};
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::thread::sleep;
    use std::time::Duration;

    const GDK_KEY_PRESS: i32 = 8;
    const GDK_KEY_F6: u32 = 0xffc3;
    const GDK_KEY_B: u32 = 0x062;

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

    use crate::{AureaResult, Window};
    use std::os::raw::c_void;

    unsafe extern "C" {
        fn gtk_widget_destroy(widget: *mut c_void);
        fn g_log_set_always_fatal(mask: u32) -> u32;
    }

    const G_LOG_LEVEL_CRITICAL: u32 = 1 << 3;

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn dropping_a_window_the_platform_already_closed_is_harmless() -> AureaResult<()> {
        unsafe { g_log_set_always_fatal(G_LOG_LEVEL_CRITICAL) };

        let window = Window::new("already closed", 200, 150)?;
        window.show();

        unsafe { gtk_widget_destroy(window.handle()) };

        drop(window);
        Ok(())
    }
}
#[cfg(all(test, windows))]
mod win32_ime_tests {
    //! Input method messages reach the application as preedit and text events.

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

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_preedit_clears_once() -> AureaResult<()> {
        let window = opened()?;
        send(&window, WM_IME_ENDCOMPOSITION, 0);
        send(&window, WM_IME_ENDCOMPOSITION, 0);

        let clears = window
            .poll_events()
            .into_iter()
            .filter(|event| matches!(event, WindowEvent::ImePreedit { .. }))
            .count();
        assert_eq!(clears, 1);
        Ok(())
    }

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

    use crate::{AureaError, AureaResult, Window};
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

        drop(first);
        drop(second);
        assert_eq!(super::lock(&super::PLATFORM).windows, 0);
        Ok(())
    }

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
mod win32_occlusion_tests {
    use crate::{AureaResult, Occlusion, Window, WindowEvent};
    use std::os::raw::{c_int, c_void};
    use std::process::{Child, Command, Stdio};
    use std::ptr::null_mut;
    use std::thread::sleep;
    use std::time::Duration;

    const SW_MINIMIZE: c_int = 6;
    const SW_RESTORE: c_int = 9;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut c_void, cmd: c_int) -> i32;
        fn SetWindowPos(
            hwnd: *mut c_void,
            after: *mut c_void,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            flags: u32,
        ) -> i32;
    }

    unsafe extern "C" {
        fn ng_windows_occlusion_level(visible: i64, total: i64) -> c_int;
    }

    #[test]
    fn levels_split_at_a_thousandth() {
        let level = |visible, total| unsafe { ng_windows_occlusion_level(visible, total) };
        assert_eq!(level(1000, 1000), 0);
        assert_eq!(level(999, 1000), 0);
        assert_eq!(level(998, 1000), 1);
        assert_eq!(level(2, 1000), 1);
        assert_eq!(level(1, 1000), 2);
        assert_eq!(level(0, 0), 0, "an empty window is not hidden");
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
    }

    /// A topmost window of another process over `(x, y, w, h)` in screen pixels, until dropped.
    fn cover(x: i32, y: i32, w: i32, h: i32) -> Child {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object Windows.Forms.Form;              $f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'; $f.TopMost = $true;              $f.ShowInTaskbar = $false; $f.Location = New-Object Drawing.Point({x},{y});              $f.Size = New-Object Drawing.Size({w},{h}); $f.ShowDialog()"
        );
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("powershell")
    }

    /// Polls until `window` says it changed to `want`, for up to five seconds.
    fn wait_for(window: &Window, want: Occlusion) -> bool {
        for _ in 0..100 {
            let heard = window.poll_events();
            if heard
                .iter()
                .any(|e| matches!(e, WindowEvent::OcclusionChanged(level) if *level == want))
            {
                return true;
            }
            sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    #[ignore = "creates native windows and a powershell window; run with --ignored"]
    fn another_apps_window_covers_part_then_all() -> AureaResult<()> {
        let window = Window::new("covered", 300, 200)?;
        window.show();
        let _ = window.poll_events();
        let mut r = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe { GetWindowRect(window.handle(), &raw mut r) };
        let (w, h) = (r.right - r.left, r.bottom - r.top);

        let mut half = cover(r.left, r.top, w / 2, h);
        let partly = wait_for(&window, Occlusion::PartlyCovered);
        let _ = half.kill();
        let _ = half.wait();
        assert!(partly, "half covered");

        let mut all = cover(r.left - 20, r.top - 20, w + 40, h + 40);
        let covered = wait_for(&window, Occlusion::Covered);
        let _ = all.kill();
        let _ = all.wait();
        assert!(covered, "all covered");
        assert!(wait_for(&window, Occlusion::Visible), "uncovered again");
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn minimized_and_off_screen_windows_are_covered() -> AureaResult<()> {
        let window = Window::new("occlusion", 300, 200)?;
        window.show();
        let _ = window.poll_events();
        assert_eq!(window.occlusion(), Occlusion::Visible);

        unsafe { ShowWindow(window.handle(), SW_MINIMIZE) };
        let heard = window.poll_events();
        assert_eq!(window.occlusion(), Occlusion::Covered);
        assert!(
            heard
                .iter()
                .any(|e| matches!(e, WindowEvent::OcclusionChanged(Occlusion::Covered))),
            "{heard:?}"
        );

        unsafe { ShowWindow(window.handle(), SW_RESTORE) };
        let _ = window.poll_events();
        assert_eq!(window.occlusion(), Occlusion::Visible);

        unsafe {
            SetWindowPos(
                window.handle(),
                null_mut(),
                -20000,
                -20000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER,
            )
        };
        assert_eq!(window.occlusion(), Occlusion::Covered);
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod win32_wheel_tests {
    use crate::{AureaResult, Window, WindowEvent};
    use std::os::raw::c_void;

    const WM_MOUSEWHEEL: u32 = 0x020A;
    const MK_SHIFT: usize = 0x0004;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
    }

    /// One notch towards the user (down), with `keys` held.
    fn wheel_down(window: &Window, keys: usize) -> Vec<(f64, f64)> {
        let notch = usize::from(120u16.wrapping_neg()) << 16;
        unsafe { SendMessageW(window.handle(), WM_MOUSEWHEEL, notch | keys, 0) };
        window
            .poll_events()
            .into_iter()
            .filter_map(|event| match event {
                WindowEvent::MouseWheel {
                    delta_x, delta_y, ..
                } => Some((delta_x, delta_y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn shift_turns_the_wheel_sideways() -> AureaResult<()> {
        let window = Window::new("wheel", 300, 200)?;
        window.show();
        let _ = window.poll_events();
        assert_eq!(wheel_down(&window, 0), [(0.0, -1.0)]);
        assert_eq!(
            wheel_down(&window, MK_SHIFT),
            [(1.0, 0.0)],
            "down goes right"
        );
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod win32_text_input_tests {
    //! Text typed in any script has to arrive intact.

    use crate::{AureaResult, Window, WindowEvent};
    use std::os::raw::c_void;

    const WM_CHAR: u32 = 0x0102;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> i32;
    }

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

        type_unit(&window, 0xD55C);
        type_unit(&window, 0x0416);

        assert_eq!(typed_text(&window), "한Ж");
        Ok(())
    }

    /// A focused canvas gets keys through the message loop, as dispatched `WM_CHAR`s.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn text_typed_into_a_canvas_arrives_intact() -> AureaResult<()> {
        use crate::elements::Element;
        use crate::render::{Canvas, RendererBackend};

        let mut window = Window::new("canvas text", 300, 200)?;
        let canvas = Canvas::new(200, 100, RendererBackend::Cpu)?;
        let handle = canvas.handle();
        window.set_content(canvas)?;
        window.show();
        let _ = window.poll_events();

        // Thai and an emoji sit outside every East Asian ANSI code page.
        for unit in "가ก😀".encode_utf16() {
            unsafe { PostMessageW(handle, WM_CHAR, usize::from(unit), 0) };
        }

        assert_eq!(typed_text(&window), "가ก😀");
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

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_surrogate_pair_arrives_as_one_character() -> AureaResult<()> {
        let window = Window::new("text", 300, 200)?;
        window.show();
        let _ = window.poll_events();

        for unit in "\u{1F600}".encode_utf16() {
            type_unit(&window, unit);
        }

        assert_eq!(typed_text(&window), "\u{1F600}");
        Ok(())
    }

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

#[cfg(test)]
mod custom_cursor_tests {
    use super::*;
    use crate::render::Image;

    #[test]
    #[ignore = "creates a native window; run on the platform UI thread"]
    fn custom_cursors_replace_release_and_validate_pixels() -> AureaResult<()> {
        let window = Window::new("cursor lifecycle", 80, 80)?;
        let image = Image::new(16, 16, [255, 0, 0, 128].repeat(256));
        assert!(window.set_cursor_image(&image, (16, 0)).is_err());
        assert!(
            window
                .set_cursor_image(&Image::new(16, 16, vec![0; 4]), (0, 0))
                .is_err()
        );
        #[cfg(windows)]
        let before = handles();
        for _ in 0..300 {
            window.set_cursor_image(&image, (8, 8))?;
            window.set_cursor_image(&image, (0, 0))?;
            window.set_cursor(CursorIcon::ResizeNwSe)?;
            window.set_cursor(CursorIcon::ResizeNeSw)?;
        }
        #[cfg(windows)]
        assert!(handles() <= before + 2, "native cursor objects leaked");
        window.set_cursor_image(&image, (8, 8))?;
        drop(window);
        #[cfg(windows)]
        assert!(
            handles() <= before + 2,
            "window destruction leaked its cursor"
        );
        Ok(())
    }

    #[cfg(windows)]
    fn handles() -> u32 {
        use std::ffi::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetGuiResources(process: *mut c_void, flags: u32) -> u32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
        }
        unsafe { GetGuiResources(GetCurrentProcess(), 1) }
    }
}
#[cfg(all(test, windows))]
mod win32_dpi_tests {
    //! A canvas on a 150% monitor and a 100% one: its buffer is the client area to the pixel, so it
    //! shows 1:1 and not stretched.

    use crate::render::{Canvas, Color, RendererBackend};
    use crate::{AureaResult, Window};
    use std::os::raw::c_void;
    use std::ptr::{null, null_mut};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    type MonitorProc = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut Rect, isize) -> i32;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(
            dc: *mut c_void,
            clip: *const Rect,
            f: MonitorProc,
            data: isize,
        ) -> i32;
        fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        fn SetWindowPos(
            hwnd: *mut c_void,
            after: *mut c_void,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            flags: u32,
        ) -> i32;
    }

    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;

    #[link(name = "shcore")]
    unsafe extern "system" {
        fn GetDpiForMonitor(monitor: *mut c_void, kind: i32, x: *mut u32, y: *mut u32) -> i32;
    }

    unsafe extern "system" fn collect(
        monitor: *mut c_void,
        _: *mut c_void,
        rect: *mut Rect,
        data: isize,
    ) -> i32 {
        let out = unsafe { &mut *(data as *mut Vec<(Rect, u32)>) };
        let (mut x, mut y) = (0, 0);
        unsafe { GetDpiForMonitor(monitor, 0, &mut x, &mut y) };
        out.push((unsafe { *rect }, x));
        1
    }

    fn monitors() -> Vec<(Rect, u32)> {
        let mut out: Vec<(Rect, u32)> = Vec::new();
        unsafe { EnumDisplayMonitors(null_mut(), null(), collect, &mut out as *mut _ as isize) };
        out
    }

    fn client_size(hwnd: *mut c_void) -> (u32, u32) {
        let mut r = Rect::default();
        unsafe { GetClientRect(hwnd, &mut r) };
        (
            (r.right - r.left).cast_unsigned(),
            (r.bottom - r.top).cast_unsigned(),
        )
    }

    fn move_to(window: &Window, monitor: Rect, scale: f32) {
        window.set_position(monitor.left + 80, monitor.top + 80);
        let deadline = Instant::now() + Duration::from_secs(3);
        while (window.scale_factor() - scale).abs() > 0.01 && Instant::now() < deadline {
            let _ = window.poll_events();
            sleep(Duration::from_millis(10));
        }
        assert!(
            (window.scale_factor() - scale).abs() <= 0.01,
            "scale {} on a {scale} monitor",
            window.scale_factor()
        );
    }

    fn check(window: &Window, canvas: &mut Canvas, what: &str) -> AureaResult<()> {
        let _ = window.poll_events();
        canvas.draw(|ctx| ctx.clear(Color::rgb(255, 0, 0)))?;
        let client = client_size(canvas.native_handle());
        if let Some((w, h, corner)) =
            canvas.with_frame_pixels(|px, w, h| (w, h, px[(w * h - 1) as usize]))
        {
            assert_eq!((w, h), client, "buffer vs client {what}");
            assert_eq!(corner & 0x00FF_FFFF, 0x00FF_0000, "last pixel {what}");
        }
        let (lw, lh) = canvas.size();
        let s = canvas.scale_factor();
        assert!(
            lw as f32 * s >= client.0 as f32 && (lw as f32 - 1.0) * s < client.0 as f32,
            "logical width {lw} covers {} at {s} {what}",
            client.0
        );
        assert!(
            lh as f32 * s >= client.1 as f32,
            "logical height {lh} {what}"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn an_oversized_window_fits_its_work_area() -> AureaResult<()> {
        #[repr(C)]
        struct MonitorInfo {
            size: u32,
            monitor: Rect,
            work: Rect,
            flags: u32,
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
            fn MonitorFromWindow(hwnd: *mut c_void, flags: u32) -> *mut c_void;
            fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        }
        const MONITOR_DEFAULTTONEAREST: u32 = 2;
        let window = Window::new("oversized", 20_000, 20_000)?;
        let mut info = MonitorInfo {
            size: u32::try_from(size_of::<MonitorInfo>()).unwrap_or(0),
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        let mut outer = Rect::default();
        unsafe {
            GetMonitorInfoW(
                MonitorFromWindow(window.handle, MONITOR_DEFAULTTONEAREST),
                &mut info,
            );
            GetWindowRect(window.handle, &mut outer);
        }
        let work = info.work;
        assert!(
            outer.left >= work.left
                && outer.top >= work.top
                && outer.right <= work.right
                && outer.bottom <= work.bottom,
            "{:?} outside {:?}",
            (outer.left, outer.top, outer.right, outer.bottom),
            (work.left, work.top, work.right, work.bottom)
        );
        Ok(())
    }

    #[test]
    #[ignore = "needs a 150% monitor; run with --ignored"]
    fn an_odd_minimum_reads_back_at_150_percent() -> AureaResult<()> {
        let window = Window::new("odd minimum", 320, 240)?;
        let Some(&(scaled, _)) = monitors().iter().find(|(_, dpi)| *dpi == 144) else {
            eprintln!("needs a 150% monitor");
            return Ok(());
        };
        window.show();
        move_to(&window, scaled, 1.5);
        window.set_min_size(333, 251)?;
        window.set_size(100, 100);
        assert_eq!(window.size(), (333, 251));
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window on two monitors; run with --ignored"]
    fn a_canvas_matches_its_client_at_150_and_100_percent() -> AureaResult<()> {
        across_monitors(RendererBackend::Cpu)
    }

    #[cfg(feature = "zengpu")]
    #[test]
    #[ignore = "creates a native window and a GPU device; run with --ignored"]
    fn a_gpu_canvas_draws_at_every_size_and_scale() -> AureaResult<()> {
        across_monitors(RendererBackend::ZenGpu)
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetObjectW(object: *mut c_void, size: i32, out: *mut c_void) -> i32;
    }

    fn font_height(hwnd: *mut c_void) -> i32 {
        const WM_GETFONT: u32 = 0x0031;
        let mut logfont = [0i32; 23];
        unsafe {
            let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as *mut c_void;
            GetObjectW(font, 92, logfont.as_mut_ptr().cast());
        }
        logfont[0].abs()
    }

    #[test]
    #[ignore = "needs a 150% and a 100% monitor; run with --ignored"]
    fn native_fonts_follow_the_monitor() -> AureaResult<()> {
        use crate::elements::{Container, Element, Label, Orientation, Stack, TextEditor};
        let mut window = Window::new("fonts", 320, 240)?;
        let all = monitors();
        let scaled = all.iter().find(|(_, dpi)| *dpi == 144);
        let plain = all.iter().find(|(_, dpi)| *dpi == 96);
        let (Some(&(scaled, _)), Some(&(plain, _))) = (scaled, plain) else {
            eprintln!("needs a 150% and a 100% monitor");
            return Ok(());
        };
        let label = Label::new("label")?;
        let editor = TextEditor::new()?;
        let (l, e) = (label.handle(), editor.handle());
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(label)?;
        root.add_weighted(editor, 1.0)?;
        window.set_content(root)?;
        window.show();
        move_to(&window, plain, 1.0);
        let (label_at_1, editor_at_1) = (font_height(l), font_height(e));
        move_to(&window, scaled, 1.5);
        for (what, before, after) in [
            ("label", label_at_1, font_height(l)),
            ("editor", editor_at_1, font_height(e)),
        ] {
            assert!(
                (after * 2 - before * 3).abs() <= 2,
                "{what} font {before} px at 100%, {after} px at 150%"
            );
        }
        Ok(())
    }

    fn across_monitors(backend: RendererBackend) -> AureaResult<()> {
        let mut window = Window::new("dpi", 320, 240)?;
        let all = monitors();
        let scaled = all.iter().find(|(_, dpi)| *dpi == 144);
        let plain = all.iter().find(|(_, dpi)| *dpi == 96);
        let (Some(&(scaled, _)), Some(&(plain, _))) = (scaled, plain) else {
            eprintln!(
                "needs a 150% and a 100% monitor; found {:?}",
                all.iter().map(|m| m.1).collect::<Vec<_>>()
            );
            return Ok(());
        };
        let mut canvas = Canvas::new(320, 240, backend)?;
        window.set_content(canvas.clone())?;
        window.show();

        move_to(&window, scaled, 1.5);
        for (lw, lh) in [(667, 401), (668, 400), (333, 211)] {
            window.set_size(lw, lh);
            assert_eq!(window.size(), (lw, lh), "a size set reads back at 150%");
            check(&window, &mut canvas, &format!("at 150%, set {lw}x{lh}"))?;
        }
        for outer in 900..903 {
            unsafe {
                SetWindowPos(
                    window.handle,
                    null_mut(),
                    0,
                    0,
                    outer,
                    outer - 200,
                    SWP_NOMOVE | SWP_NOZORDER,
                )
            };
            let (w, h) = window.size();
            let client = client_size(window.handle);
            assert!(
                w as f32 * 1.5 >= client.0 as f32 && h as f32 * 1.5 >= client.1 as f32,
                "window size {w}x{h} covers its {client:?} client"
            );
            check(&window, &mut canvas, &format!("at 150%, outer {outer}"))?;
        }

        move_to(&window, plain, 1.0);
        window.set_size(667, 401);
        check(&window, &mut canvas, "at 100%")?;

        move_to(&window, scaled, 1.5);
        check(&window, &mut canvas, "back at 150%")?;
        Ok(())
    }
}
#[cfg(all(test, target_os = "linux"))]
mod gtk_ime_tests {
    use crate::elements::Label;
    use crate::{AureaResult, Window};
    use std::os::raw::{c_int, c_void};

    unsafe extern "C" {
        fn ng_linux_content_origin(window: *mut c_void, x: *mut c_int, y: *mut c_int);
        fn gtk_container_check_resize(container: *mut c_void);
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn the_caret_area_counts_from_below_the_menu_bar() -> AureaResult<()> {
        let origin = |menu: bool| -> AureaResult<(c_int, c_int)> {
            let mut window = Window::new("ime menu", 300, 200)?;
            if menu {
                window
                    .create_menu_bar()?
                    .add_submenu("File")?
                    .add_item("Quit", || {})?;
            }
            window.set_content(Label::new("body")?)?;
            window.show();
            let _ = window.poll_events();
            // Mapping can precede GTK's next layout tick. Compare allocated
            // content origins, not a child's initial (-1, -1) allocation.
            unsafe { gtk_container_check_resize(window.handle()) };
            let (mut x, mut y) = (0, 0);
            unsafe { ng_linux_content_origin(window.handle(), &mut x, &mut y) };
            Ok((x, y))
        };
        let (plain_x, plain_y) = origin(false)?;
        let (menu_x, menu_y) = origin(true)?;
        assert_eq!(menu_x, plain_x);
        assert!(
            menu_y > plain_y,
            "menu bar pushes content down: {plain_y} -> {menu_y}"
        );
        Ok(())
    }
}
