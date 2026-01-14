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
use std::{ffi::CString, os::raw::c_void, sync::Mutex};

use log::info;

pub struct Window {
    pub handle: *mut c_void,
    pub menu_bar: Option<MenuBar>,
    pub content: Option<Box<dyn Element>>,
    platform: Platform,
    capabilities: CapabilityChecker,
    damage: Mutex<DamageRegion>,
    scale_factor: Mutex<f32>,
}

impl Window {
    pub fn new(title: &str, width: i32, height: i32) -> AureaResult<Self> {
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
        let handle = unsafe { ng_platform_create_window(title.as_ptr(), width, height) };

        if handle.is_null() {
            return Err(AureaError::WindowCreationFailed);
        }

        let scale_factor = unsafe { ng_platform_get_scale_factor(handle) };

        Ok(Self {
            handle,
            menu_bar: None,
            content: None,
            platform,
            capabilities,
            damage: Mutex::new(DamageRegion::new(16)),
            scale_factor: Mutex::new(scale_factor),
        })
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
}

impl Drop for Window {
    fn drop(&mut self) {
        unregister_lifecycle_callback(self.handle);

        unsafe {
            ng_platform_destroy_window(self.handle);
            ng_platform_cleanup();
        }
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}
