//! Pads without a backend: no `gamepad` feature, or a system without one. `Pads` finds none.

use super::{PadEvent, PadInfo};

/// Game controllers. This build or system has no backend, so no pad is ever listed.
#[derive(Debug, Default)]
pub struct Pads;

impl Pads {
    /// An empty set of pads.
    pub fn new() -> Pads {
        Pads
    }

    /// Adds nothing: there are no pads.
    pub fn poll(&mut self, _events: &mut Vec<PadEvent>) {}

    /// Always empty.
    pub fn list(&self) -> &[PadInfo] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::Pads;

    #[test]
    fn mirror_is_empty() {
        let mut pads = Pads::new();
        let mut events = Vec::new();
        pads.poll(&mut events);
        assert!(events.is_empty());
        assert!(pads.list().is_empty());
    }
}
