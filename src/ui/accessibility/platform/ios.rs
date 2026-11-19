//! VoiceOver for the drawn UI on iOS. AccessKit has no iOS adapter, so each tree update goes to
//! the host view as the flat list of elements UIKit takes; what VoiceOver asks comes back as
//! AccessKit actions on the same nodes.

use std::collections::HashSet;
use std::ffi::{CString, c_int};
use std::ptr;
use std::sync::Mutex;

use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, NodeId, TreeId, TreeUpdate,
};
use aurea_foundation::lock;

use super::super::flat::flatten;
use super::super::tree::Handler;
use crate::ffi::{NGAxElement, ng_ios_ax_set_handler, ng_ios_ax_update};
use crate::render::Canvas;
use crate::{AureaError, AureaResult, Platform};

/// The screen's handler, and which elements click when activated (the others take focus).
struct Live {
    handler: Handler,
    clicks: HashSet<u64>,
}

/// One screen, so one bridge at a time.
static LIVE: Mutex<Option<Live>> = Mutex::new(None);

unsafe extern "C" fn on_action(id: u64, kind: c_int) {
    let Some((mut handler, clicks)) = lock(&LIVE)
        .as_ref()
        .map(|live| (live.handler.clone(), live.clicks.contains(&id)))
    else {
        return;
    };
    let action = match kind {
        1 => Action::Increment,
        2 => Action::Decrement,
        _ if clicks => Action::Click,
        _ => Action::Focus,
    };
    handler.do_action(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: NodeId(id),
        data: None,
    });
}

unsafe extern "C" fn on_activate() {
    let handler = lock(&LIVE).as_ref().map(|live| live.handler.clone());
    if let Some(mut handler) = handler {
        // the tree comes with the next frame
        let _ = handler.request_initial_tree();
    }
}

pub(in crate::ui::accessibility) struct Adapter;

impl Adapter {
    pub(in crate::ui::accessibility) fn new(_: &Canvas, _: Handler) -> AureaResult<Self> {
        Err(AureaError::Unsupported {
            operation: "canvas_accessibility",
            platform: Platform::current(),
        })
    }

    /// The bridge for the screen the app draws.
    pub(in crate::ui::accessibility) fn host(handler: Handler) -> Self {
        *lock(&LIVE) = Some(Live {
            handler,
            clicks: HashSet::new(),
        });
        // may ask for the tree at once, when VoiceOver is already on
        unsafe { ng_ios_ax_set_handler(Some(on_action), Some(on_activate)) };
        Self
    }

    pub(in crate::ui::accessibility) fn update(&mut self, update: TreeUpdate) {
        let elements = flatten(&update);
        if let Some(live) = lock(&LIVE).as_mut() {
            live.clicks = elements.iter().filter(|e| e.clicks).map(|e| e.id).collect();
        }
        let text = |s: &str| CString::new(s.replace('\0', "")).unwrap_or_default();
        let strings: Vec<[CString; 3]> = elements
            .iter()
            .map(|e| [text(&e.label), text(&e.value), text(&e.hint)])
            .collect();
        let native: Vec<NGAxElement> = elements
            .iter()
            .zip(&strings)
            .map(|(e, [label, value, hint])| NGAxElement {
                id: e.id,
                x: e.frame[0],
                y: e.frame[1],
                width: e.frame[2],
                height: e.frame[3],
                traits: e.traits,
                label: label.as_ptr(),
                value: value.as_ptr(),
                hint: hint.as_ptr(),
            })
            .collect();
        unsafe { ng_ios_ax_update(native.as_ptr(), native.len(), update.focus.0) };
    }

    pub(in crate::ui::accessibility) fn focused(&mut self, _: bool) {}
}

impl Drop for Adapter {
    fn drop(&mut self) {
        *lock(&LIVE) = None;
        unsafe {
            ng_ios_ax_set_handler(None, None);
            ng_ios_ax_update(ptr::null(), 0, 0);
        }
    }
}
