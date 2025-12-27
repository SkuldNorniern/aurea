use std::{ffi::CString, os::raw::c_void, sync::{Mutex, LazyLock}, collections::HashMap};
use crate::{AureaError, AureaResult, ffi::*};
use super::traits::Element;

static BUTTON_CALLBACKS: LazyLock<Mutex<HashMap<u32, Box<dyn Fn() + Send + Sync>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct Button {
    handle: *mut c_void,
    _title: CString,
    _id: u32,
}

impl Button {
    pub fn new(title: &str) -> AureaResult<Self> {
        Self::with_callback(title, || {})
    }

    pub fn with_callback<F>(title: &str, callback: F) -> AureaResult<Self>
    where
        F: Fn() + Send + Sync + 'static,
    {
        static BUTTON_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
        let id = {
            let mut id_guard = BUTTON_ID.lock().unwrap();
            *id_guard += 1;
            *id_guard - 1
        };

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_button(title.as_ptr(), id) };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut callbacks = BUTTON_CALLBACKS.lock().unwrap();
        callbacks.insert(id, Box::new(callback));
        
        Ok(Self {
            handle,
            _title: title,
            _id: id,
        })
    }
}

pub(crate) fn invoke_button_callback(id: u32) {
    let callbacks = BUTTON_CALLBACKS.lock().unwrap();
    if let Some(callback) = callbacks.get(&id) {
        callback();
    }
}

impl Element for Button {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
    
    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_button_invalidate(self.handle);
        }
    }
}

