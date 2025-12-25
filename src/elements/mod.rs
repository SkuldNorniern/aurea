use std::{ffi::CString, os::raw::c_void};

use crate::{AureaError, AureaResult, ffi::*};

/// Represents a basic GUI element
pub trait Element {
    /// Returns the native handle for the element
    fn handle(&self) -> *mut c_void;
}

/// A container element that can hold other elements
pub trait Container: Element {
    /// Adds a child element to this container
    fn add<E: Element>(&mut self, element: E) -> AureaResult<()>;
}

/// Basic element properties
#[derive(Debug, Clone)]
pub struct ElementProps<'a> {
    pub title: &'a str,
    pub width: i32,
    pub height: i32,
}

/// A basic button element
pub struct Button {
    handle: *mut c_void,
    _title: CString, // Keep the CString alive while the button exists
}

impl Button {
    pub fn new(title: &str) -> AureaResult<Self> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        
        let handle = unsafe { 
            ng_platform_create_button(title.as_ptr())
        };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self {
            handle,
            _title: title,
        })
    }
}

impl Element for Button {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

/// A basic label element
pub struct Label {
    handle: *mut c_void,
    _text: CString,
}

impl Label {
    pub fn new(text: &str) -> AureaResult<Self> {
        let text = CString::new(text).map_err(|_| AureaError::InvalidTitle)?;
        
        let handle = unsafe { 
            ng_platform_create_label(text.as_ptr())
        };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self {
            handle,
            _text: text,
        })
    }
}

impl Element for Label {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

/// Box orientation
#[derive(Debug, Clone, Copy)]
pub enum BoxOrientation {
    Horizontal,
    Vertical,
}

/// A container that can hold multiple elements
pub struct Box {
    handle: *mut c_void,
    _orientation: BoxOrientation,
}

impl Box {
    pub fn new(orientation: BoxOrientation) -> AureaResult<Self> {
        let is_vertical = match orientation {
            BoxOrientation::Vertical => 1,
            BoxOrientation::Horizontal => 0,
        };
        
        let handle = unsafe { 
            ng_platform_create_box(is_vertical)
        };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self {
            handle,
            _orientation: orientation,
        })
    }
}

impl Element for Box {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl Container for Box {
    fn add<E: Element>(&mut self, element: E) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_box_add(self.handle, element.handle())
        };
        
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(())
    }
}

/// A text editor component suitable for editing plain text
pub struct TextEditor {
    handle: *mut c_void,
}

impl TextEditor {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_text_editor() };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self { handle })
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

/// A text view component that can be either editable or read-only
pub struct TextView {
    handle: *mut c_void,
}

impl TextView {
    pub fn new(editable: bool) -> AureaResult<Self> {
        let handle = unsafe { 
            ng_platform_create_text_view(if editable { 1 } else { 0 })
        };
        
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        
        Ok(Self { handle })
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

impl Element for TextView {
    fn handle(&self) -> *mut c_void {
        self.handle
    }
}

// Drop implementations
// Note: On macOS, elements use __bridge_retained and are managed by their parent views.
// When removed from the view hierarchy, ARC handles deallocation automatically.
// Explicit cleanup is not needed as the native views manage their own lifecycle.
impl Drop for TextEditor {
    fn drop(&mut self) {
        // Native view lifecycle managed by parent container
    }
}

impl Drop for TextView {
    fn drop(&mut self) {
        // Native view lifecycle managed by parent container
    }
}

impl Drop for Button {
    fn drop(&mut self) {
        // Native view lifecycle managed by parent container
    }
}

impl Drop for Label {
    fn drop(&mut self) {
        // Native view lifecycle managed by parent container
    }
}

impl Drop for Box {
    fn drop(&mut self) {
        // Native view lifecycle managed by parent container
    }
}
