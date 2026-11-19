//! TalkBack for the drawn UI on Android. `accesskit_android` injects its accessibility delegate
//! into the host view, so no view subclass is needed; TalkBack's requests come back as actions on
//! the same nodes. The delegate class is `dev.accesskit.android.Delegate`, shipped in the `aurea`
//! Gradle module.

use accesskit::TreeUpdate;
use accesskit_android::InjectingAdapter;

use super::super::tree::Handler;
use crate::android::with_host_view;
use crate::render::Canvas;
use crate::{AureaError, AureaResult, Platform};

pub(in crate::ui::accessibility) struct Adapter(InjectingAdapter);

impl Adapter {
    pub(in crate::ui::accessibility) fn new(_: &Canvas, _: Handler) -> AureaResult<Self> {
        Err(AureaError::Unsupported {
            operation: "canvas_accessibility",
            platform: Platform::current(),
        })
    }

    /// The bridge for the screen the app draws, on the Activity's surface view.
    pub(in crate::ui::accessibility) fn host(handler: Handler) -> AureaResult<Self> {
        with_host_view(|env, view| Self(InjectingAdapter::new(env, view, handler.clone(), handler)))
            .ok_or(AureaError::ElementOperationFailed)
    }

    pub(in crate::ui::accessibility) fn update(&mut self, update: TreeUpdate) {
        self.0.update_if_active(|| update);
    }

    pub(in crate::ui::accessibility) fn focused(&mut self, _: bool) {}
}
