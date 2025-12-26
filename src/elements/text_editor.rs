use std::{ffi::CString, os::raw::c_void, sync::{Mutex, LazyLock}, collections::HashMap};
use crate::{AureaError, AureaResult, ffi::*};
use super::traits::Element;

static TEXT_CALLBACKS: LazyLock<Mutex<HashMap<u32, Box<dyn Fn(String) + Send + Sync>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct TextEditor {
    handle: *mut c_void,
    _id: u32,
}

impl TextEditor {
    pub fn new() -> AureaResult<Self> {
        Self::with_callback(|_| {})
    }

    pub fn with_callback<F>(callback: F) -> AureaResult<Self>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        static TEXT_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(1));
        let id = {
            let mut id_guard = TEXT_ID.lock().unwrap();
            *id_guard += 1;
            *id_guard - 1
        };

        let handle = unsafe { ng_platform_create_text_editor(id) };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut callbacks = TEXT_CALLBACKS.lock().unwrap();
        callbacks.insert(id, Box::new(callback));
        
        Ok(Self { handle, _id: id })
    }

    pub fn set_content(&mut self, content: &str) -> AureaResult<()> {
        let content = CString::new(content).map_err(|_| AureaError::InvalidTitle)?;
        let result = unsafe {
            ng_platform_set_text_content(self.handle, content.as_ptr())
        };
        
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(())
    }

    pub fn get_content(&self) -> AureaResult<String> {
        let content_ptr = unsafe { ng_platform_get_text_content(self.handle) };
        
        if content_ptr.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        let content = unsafe {
            let cstr = std::ffi::CStr::from_ptr(content_ptr);
            let result = cstr.to_str()
                .map_err(|_| AureaError::ElementOperationFailed)?
                .to_string();
            ng_platform_free_text_content(content_ptr);
            result
        };
        
        Ok(content)
    }
}

impl Element for TextEditor {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

pub(crate) fn invoke_text_callback(id: u32, content: String) {
    let callbacks = TEXT_CALLBACKS.lock().unwrap();
    if let Some(callback) = callbacks.get(&id) {
        callback(content);
    }
}

