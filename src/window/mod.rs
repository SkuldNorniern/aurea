mod events;
mod manager;

pub use events::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};
pub use manager::WindowManager;

/// Window type for different window behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Standard application window with title bar, minimize/maximize buttons
    Normal,
    /// Popup window (borderless or minimal border, stays on top)
    Popup,
    /// Tool window (floating, smaller title bar, stays on top of parent)
    Tool,
    /// Utility window (similar to tool, but different styling)
    Utility,
    /// Sheet window (modal, attached to parent window - macOS)
    Sheet,
    /// Dialog window (modal dialog)
    Dialog,
}

use crate::capability::{Capability, CapabilityChecker};
use crate::elements::Element;
use crate::ffi::*;
use crate::lifecycle::{
    LifecycleEvent, register_lifecycle_callback, unregister_lifecycle_callback,
};
use crate::menu::MenuBar;
use crate::platform::Platform;
use crate::view::{DamageRegion, FrameScheduler};
use crate::{AureaError, AureaResult};
use std::{
    collections::HashMap,
    ffi::CString,
    os::raw::c_void,
    sync::{Arc, LazyLock, Mutex, Weak},
};

static ALL_EVENT_QUEUES: LazyLock<Mutex<Vec<Weak<events::EventQueue>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static EVENT_QUEUE_REGISTRY: LazyLock<Mutex<HashMap<usize, Weak<events::EventQueue>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn register_event_queue(handle: *mut c_void, queue: &Arc<events::EventQueue>) {
    let mut registry = EVENT_QUEUE_REGISTRY.lock().unwrap();
    registry.insert(handle as usize, Arc::downgrade(queue));
}

fn unregister_event_queue(handle: *mut c_void) {
    let mut registry = EVENT_QUEUE_REGISTRY.lock().unwrap();
    registry.remove(&(handle as usize));
}

pub(crate) fn push_window_event(handle: *mut c_void, event: WindowEvent) {
    let queue = {
        let mut registry = EVENT_QUEUE_REGISTRY.lock().unwrap();
        match registry
            .get(&(handle as usize))
            .and_then(|weak| weak.upgrade())
        {
            Some(queue) => Some(queue),
            None => {
                registry.remove(&(handle as usize));
                None
            }
        }
    };

    if let Some(queue) = queue {
        queue.push(event);
    }
}

pub(crate) fn process_all_window_events() {
    let mut queues = ALL_EVENT_QUEUES.lock().unwrap();
    queues.retain(|weak| {
        if let Some(queue) = weak.upgrade() {
            queue.process_events();
            true
        } else {
            false
        }
    });
}

use log::info;

pub struct Window {
    pub handle: *mut c_void,
    pub menu_bar: Option<MenuBar>,
    pub content: Option<Box<dyn Element>>,
    platform: Platform,
    capabilities: CapabilityChecker,
    damage: Mutex<DamageRegion>,
    scale_factor: Mutex<f32>,
    event_queue: Arc<events::EventQueue>,
    window_type: WindowType,
}

impl Window {
    /// Create a new window with default type (Normal)
    pub fn new(title: &str, width: i32, height: i32) -> AureaResult<Self> {
        Self::with_type(title, width, height, WindowType::Normal)
    }

    /// Create a new window with specified type
    pub fn with_type(
        title: &str,
        width: i32,
        height: i32,
        window_type: WindowType,
    ) -> AureaResult<Self> {
        static INIT: std::sync::Once = std::sync::Once::new();
        let mut error = None;

        INIT.call_once(|| {
            if unsafe { ng_platform_init() } != 0 {
                error = Some(AureaError::PlatformError(1));
            }
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
        let event_queue = Arc::new(events::EventQueue::new());

        {
            let mut queues = ALL_EVENT_QUEUES.lock().unwrap();
            queues.push(Arc::downgrade(&event_queue));
        }

        register_event_queue(handle, &event_queue);

        // Register lifecycle bridge
        let eq_clone = event_queue.clone();
        let handle_usize = handle as usize;
        register_lifecycle_callback(
            handle,
            Box::new(move |event| {
                let handle_ptr = handle_usize as *mut std::os::raw::c_void;
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
                            width: w as u32,
                            height: h as u32,
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
        }

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

    pub fn run(self) -> AureaResult<()> {
        let result = unsafe { ng_platform_run() };
        if result != 0 {
            return Err(AureaError::EventLoopError);
        }
        Ok(())
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

    pub fn add_damage(&self, rect: crate::render::Rect) {
        let mut damage = self.damage.lock().unwrap();
        damage.add(rect);
        self.schedule_frame();
    }

    pub fn take_damage(&self) -> Option<crate::render::Rect> {
        let mut damage = self.damage.lock().unwrap();
        damage.take()
    }

    pub fn scale_factor(&self) -> f32 {
        *self.scale_factor.lock().unwrap()
    }

    pub fn update_scale_factor(&self) {
        let new_scale = unsafe { ng_platform_get_scale_factor(self.handle) };
        *self.scale_factor.lock().unwrap() = new_scale;
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
    pub fn native_handle(&self) -> crate::integration::NativeWindowHandle {
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
        // Process events through callbacks and return them for manual processing
        self.event_queue.process_events()
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
        (width as u32, height as u32)
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

    /// Get the native window handle
    pub fn handle(&self) -> *mut std::ffi::c_void {
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

        unsafe {
            ng_platform_destroy_window(self.handle);
            ng_platform_cleanup();
        }
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}
