//! Touches without the `touch` feature: none are read, so the host keeps no touch state.

use std::time::Instant;

use crate::{Point, WindowEvent};

#[derive(Debug, Default)]
pub(crate) struct Touches;

impl Touches {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn waiting(&self) -> bool {
        false
    }

    pub(crate) fn native(
        &mut self,
        _: i32,
        _: usize,
        _: Point,
        _: bool,
        _: Instant,
    ) -> Option<Vec<WindowEvent>> {
        None
    }

    pub(crate) fn tick(&mut self, _: Instant) -> Vec<WindowEvent> {
        Vec::new()
    }
}
