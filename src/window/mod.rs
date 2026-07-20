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
use crate::ffi::*;
use crate::ffi::ng_platform_request_frame;
#[cfg(feature = "wgpu")]
use crate::integration::NativeWindowHandle;
use crate::lifecycle::{
    LifecycleEvent, register_lifecycle_callback, unregister_lifecycle_callback,
};
use crate::menu::MenuBar;
use crate::registry::window::{
    process_window_updates, register_event_queue, register_global_event_queue,
    register_update_callback, register_update_callbacks, unregister_event_queue,
    unregister_update_callbacks,
};
use crate::render::Rect;
use crate::sync::lock;
use aurea_runtime::{DamageRegion, FrameScheduler};
use crate::{AureaError, AureaResult};
use aurea_foundation::Platform;
use aurea_foundation::{Capability, CapabilityChecker};
use std::{
    ffi::CString,
    os::raw::c_void,
    sync::{
        Arc, Mutex, Once,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Number of live `Window`s. The platform is only torn down via
/// `ng_platform_cleanup()` when the last window is dropped — calling it while
/// other windows are still alive would destroy shared platform state (e.g.
/// `UnregisterClassA` on Windows) out from under them.
static WINDOW_COUNT: AtomicUsize = AtomicUsize::new(0);

pub use crate::registry::window::{
    process_all_window_events, process_all_window_updates, push_window_event,
};

use log::info;

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

        static INIT: Once = Once::new();
        let mut error = None;

        INIT.call_once(|| {
            let got = unsafe { ng_platform_get_abi_version() };
            if got != AUREA_FFI_ABI_VERSION {
                error = Some(AureaError::AbiVersionMismatch {
                    expected: AUREA_FFI_ABI_VERSION,
                    got,
                });
                return;
            }
            if unsafe { ng_platform_init() } != 0 {
                error = Some(AureaError::PlatformError(1));
                return;
            }
            FrameScheduler::set_request_frame_hook(|| {
                unsafe { ng_platform_request_frame() };
            });
        });

        if let Some(err) = error {
            return Err(err);
        }

        let platform = Platform::current();
        let capabilities = CapabilityChecker::new();

        info!("Creating window: {}x{}", width, height);

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

        let scale_factor = unsafe { ng_platform_get_scale_factor(handle) };
        let event_queue = Arc::new(EventQueue::new());

        register_global_event_queue(&event_queue);
        register_event_queue(handle, &event_queue);
        register_update_callbacks(handle);

        // Register lifecycle bridge
        let eq_clone = event_queue.clone();
        let handle_usize = handle as usize;
        register_lifecycle_callback(
            handle,
            Box::new(move |event| {
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

        WINDOW_COUNT.fetch_add(1, Ordering::Relaxed);

        Ok(Self {
            handle,
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
        WindowId::from_handle(self.handle)
    }

    pub fn run(&self) -> AureaResult<()> {
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

    pub fn set_content<E>(&mut self, element: E) -> AureaResult<()>
    where
        E: Element + 'static,
    {
        let content_handle = element.handle();
        if content_handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let result = unsafe { ng_platform_set_window_content(self.handle, content_handle) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        self.content = Some(Box::new(element));
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
        register_lifecycle_callback(window_handle, Box::new(callback));

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
    #[cfg(feature = "wgpu")]
    pub fn native_handle(&self) -> NativeWindowHandle {
        use crate::integration::wgpu::WindowNativeHandle;
        WindowNativeHandle::native_handle_impl(self)
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
        // Pump the native message queue first: on Windows this drives
        // PeekMessage/DispatchMessage, which runs WndProc (filling the event
        // queue) and delivers WM_PAINT for any invalidated canvas. Without
        // this, a manual poll loop never repaints and never receives input.
        unsafe { ng_platform_poll_events() };

        // Process events through callbacks and return them for manual processing
        let events = self.event_queue.process_events();
        for event in &events {
            if let WindowEvent::ScaleFactorChanged { scale_factor } = event {
                *lock(&self.scale_factor) = *scale_factor;
            }
        }
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
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        self.event_queue.register_callback(Arc::new(callback));
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
        let title_cstr = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        unsafe {
            ng_platform_window_set_title(self.handle, title_cstr.as_ptr());
        }
        Ok(())
    }

    /// Set the native application/window icon from tightly packed RGBA8 pixels.
    pub fn set_icon_rgba(&self, rgba: &[u8], width: u32, height: u32) -> AureaResult<()> {
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
        let result =
            unsafe { ng_platform_window_set_cursor_visible(self.handle, i32::from(visible)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Set cursor grab mode for this window
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> AureaResult<()> {
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
        unsafe {
            ng_platform_window_show(self.handle);
        }
    }

    /// Hide the window (without destroying it)
    pub fn hide(&self) {
        unsafe {
            ng_platform_window_hide(self.handle);
        }
    }

    /// Check if the window is visible
    pub fn is_visible(&self) -> bool {
        unsafe { ng_platform_window_is_visible(self.handle) != 0 }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unregister_lifecycle_callback(self.handle);
        unregister_event_queue(self.handle);
        unregister_update_callbacks(self.handle);

        unsafe {
            ng_platform_destroy_window(self.handle);
        }

        if WINDOW_COUNT.fetch_sub(1, Ordering::Relaxed) == 1 {
            // Last window — safe to tear down shared platform state.
            unsafe { ng_platform_cleanup() };
        }
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

#[cfg(test)]
mod event_queue_tests {
    use super::*;

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
    fn event_queue_process_events_invokes_callbacks() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        let received = Arc::new(Mutex::new(Vec::new()));
        let rec = Arc::clone(&received);
        queue.register_callback(Arc::new(move |e| {
            lock(&rec).push(e);
        }));
        let processed = queue.process_events();
        assert_eq!(processed.len(), 1);
        assert_eq!(lock(&received).len(), 1);
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
