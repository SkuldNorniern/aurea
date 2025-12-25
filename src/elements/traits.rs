use std::os::raw::c_void;
use crate::{AureaError, AureaResult};

pub trait Element {
    fn handle(&self) -> *mut c_void;
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

