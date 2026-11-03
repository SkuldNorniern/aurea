//! The system alert. It does not block, as a desktop message box does: the app keeps drawing, and
//! `done` hears the answer on the UI thread, with a frame requested after. Call on the UI thread.

use std::cell::RefCell;
use std::collections::HashMap;

use jni::JNIEnv;
use jni::objects::{JClass, JValue};
use jni::sys::{jint, jlong};

use super::with_activity;
use crate::dialog::{MessageButtons, MessageChoice};
use crate::sheets::in_turn;
use crate::{AureaError, AureaResult};

type Answered = Box<dyn FnOnce(MessageChoice)>;

thread_local! {
    static WAITING: RefCell<(i64, HashMap<i64, Answered>)> = RefCell::new((0, HashMap::new()));
}

/// Asks `title` and `message` with `buttons`; `done` hears which was tapped. The alert cannot be
/// dismissed without an answer.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when it cannot show; `done` is then not called.
pub fn alert(
    title: &str,
    message: &str,
    buttons: MessageButtons,
    done: impl FnOnce(MessageChoice) + 'static,
) -> AureaResult<()> {
    let (title, message) = (title.to_string(), message.to_string());
    in_turn(MessageChoice::Cancel, done, move |done| {
        show_alert(&title, &message, buttons, done)
    })
}

fn show_alert(
    title: &str,
    message: &str,
    buttons: MessageButtons,
    done: impl FnOnce(MessageChoice) + 'static,
) -> AureaResult<()> {
    let id = WAITING.with_borrow_mut(|(next, waiting)| {
        *next += 1;
        waiting.insert(*next, Box::new(done));
        *next
    });
    let shown = with_activity(|env, activity| {
        let title = env.new_string(title).ok()?;
        let message = env.new_string(message).ok()?;
        let called = env.call_method(
            activity,
            "alertFromNative",
            "(Ljava/lang/String;Ljava/lang/String;IJ)Z",
            &[
                JValue::Object(&title),
                JValue::Object(&message),
                JValue::Int(buttons.code()),
                JValue::Long(id),
            ],
        );
        called.and_then(|v| v.z()).ok()
    })
    .flatten()
    .unwrap_or(false);
    if shown {
        return Ok(());
    }
    WAITING.with_borrow_mut(|(_, waiting)| waiting.remove(&id));
    Err(AureaError::ElementOperationFailed)
}

#[unsafe(no_mangle)]
extern "system" fn Java_dev_aurea_AureaActivity_nativeAlertDone(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
    answer: jint,
) {
    let done = WAITING.with_borrow_mut(|(_, waiting)| waiting.remove(&id));
    if let Some(done) = done {
        done(MessageChoice::from_code(answer).unwrap_or(MessageChoice::Cancel));
    }
    super::request_frame();
}
