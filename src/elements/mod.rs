use crate::{Error, Result};
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

/// Represents a basic GUI element
pub trait Element {
    /// Returns the native handle for the element
    fn handle(&self) -> *mut c_void;
}

/// A container element that can hold other elements
pub trait Container: Element {
    /// Adds a child element to this container
    fn add<E: Element>(&mut self, element: E) -> Result<()>;
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
    pub fn new(title: &str) -> Result<Self> {
        let title = CString::new(title).map_err(|_| Error::InvalidTitle)?;
        
        let handle = unsafe { 
            crate::ng_platform_create_button(title.as_ptr())
        };
        
        if handle.is_null() {
            return Err(Error::ElementOperationFailed);
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
    pub fn new(text: &str) -> Result<Self> {
        let text = CString::new(text).map_err(|_| Error::InvalidTitle)?;
        
        let handle = unsafe { 
            crate::ng_platform_create_label(text.as_ptr())
        };
        
        if handle.is_null() {
            return Err(Error::ElementOperationFailed);
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
    pub fn new(orientation: BoxOrientation) -> Result<Self> {
        let is_vertical = match orientation {
            BoxOrientation::Vertical => 1,
            BoxOrientation::Horizontal => 0,
        };
        
        let handle = unsafe { 
            crate::ng_platform_create_box(is_vertical)
        };
        
        if handle.is_null() {
            return Err(Error::ElementOperationFailed);
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
    fn add<E: Element>(&mut self, element: E) -> Result<()> {
        let result = unsafe {
            crate::ng_platform_box_add(self.handle, element.handle())
        };
        
        if result != 0 {
            return Err(Error::ElementOperationFailed);
        }
        
        Ok(())
    }
}

// Implement Drop for all elements to ensure proper cleanup
impl Drop for Button {
    fn drop(&mut self) {
        // TODO: Add proper cleanup if needed
    }
}

impl Drop for Label {
    fn drop(&mut self) {
        // TODO: Add proper cleanup if needed
    }
}

impl Drop for Box {
    fn drop(&mut self) {
        // TODO: Add proper cleanup if needed
    }
}
