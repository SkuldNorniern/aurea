//! Without `system-fonts`: no installed fonts, so no lookup finds one.

use super::Located;
use crate::text::platform::FontRef;
use std::path::PathBuf;

/// The installed fonts: none.
#[derive(Default)]
pub(crate) struct Installed;

impl Installed {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn find(&self, _font: FontRef) -> Option<Located> {
        None
    }

    pub(crate) fn families(&self) -> Vec<String> {
        Vec::new()
    }

    pub(crate) fn fallbacks(&self) -> &[PathBuf] {
        &[]
    }
}
