use crate::render::Rect;
use crate::AureaResult;
use std::os::raw::c_void;

pub trait Element {
    fn handle(&self) -> *mut c_void;

    fn invalidate(&self, rect: Option<Rect>) {
        if let Some(r) = rect {
            self.invalidate_rect(r);
        } else {
            self.invalidate_all();
        }
    }

    fn invalidate_all(&self) {
        unsafe {
            self.invalidate_platform(None);
        }
    }

    fn invalidate_rect(&self, rect: Rect) {
        unsafe {
            self.invalidate_platform(Some(rect));
        }
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>);

    fn request_layout(&self) {}
}

pub trait Container: Element {
    fn add<E: Element>(&mut self, element: E) -> AureaResult<()>;
}

#[derive(Debug, Clone)]
pub struct ElementProps<'a> {
    pub title: &'a str,
    pub width: i32,
    pub height: i32,
}
