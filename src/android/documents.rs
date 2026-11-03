//! The system document picker and share sheet. Neither blocks: `done` hears the paths on the UI
//! thread once the user is through, with none when they cancelled. Call on the UI thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use jni::JNIEnv;
use jni::errors::Result;
use jni::objects::{JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::jlong;

use super::with_activity;
use crate::sheets::in_turn;
use crate::{AureaError, AureaResult, Rect};

type Done = Box<dyn FnOnce(Vec<PathBuf>)>;

thread_local! {
    static WAITING: RefCell<(i64, HashMap<i64, Done>)> = RefCell::new((0, HashMap::new()));
}

/// Files to open, of `extensions` (any file when empty), or several with `multiple`. The chosen
/// files are copies in the app's cache, free to read and keep. `done` runs on the UI thread, then
/// a frame is requested.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when the picker cannot show; `done` is then not called.
pub fn pick_files(
    extensions: &[&str],
    multiple: bool,
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let extensions: Vec<String> = extensions.iter().map(ToString::to_string).collect();
    in_turn(Vec::new(), done, move |done| {
        let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
        show_pick_files(&extensions, multiple, done)
    })
}

fn show_pick_files(
    extensions: &[&str],
    multiple: bool,
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let extensions: Vec<&str> = extensions
        .iter()
        .map(|e| e.trim_start_matches('.'))
        .collect();
    let id = wait(done);
    let shown = call(
        &extensions,
        "pickFilesFromNative",
        "([Ljava/lang/String;ZJ)Z",
        &[JValue::Bool(u8::from(multiple)), JValue::Long(id)],
    );
    shown_or_forget(shown, id)
}

/// Saves copies of `paths` outside the app: one file picks its name, several pick a folder. `done`
/// hears where they went, as `content:` URIs, on the UI thread; a frame is requested after it.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when `paths` is empty or the picker cannot show; `done`
/// is then not called.
pub fn export_files(
    paths: &[PathBuf],
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let paths = paths.to_vec();
    in_turn(Vec::new(), done, move |done| {
        show_export_files(&paths, done)
    })
}

fn show_export_files(
    paths: &[PathBuf],
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let id = wait(done);
    let paths = texts(paths);
    let shown = call(
        &paths,
        "exportFilesFromNative",
        "([Ljava/lang/String;J)Z",
        &[JValue::Long(id)],
    );
    shown_or_forget(shown, id)
}

/// The share sheet for `paths`. `from` is for the iPad and ignored here. Android does not say
/// whether the files went anywhere, so `done` hears true once the sheet is up.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when `paths` is empty or the sheet cannot show; `done`
/// is then not called. `done` runs on the UI thread, before this returns; no frame is requested.
pub fn share_files(
    paths: &[PathBuf],
    from: Option<Rect>,
    done: impl FnOnce(bool) + 'static,
) -> AureaResult<()> {
    let paths = paths.to_vec();
    in_turn(false, done, move |done| {
        show_share_files(&paths, from, done)
    })
}

fn show_share_files(
    paths: &[PathBuf],
    _from: Option<Rect>,
    done: impl FnOnce(bool) + 'static,
) -> AureaResult<()> {
    let paths = texts(paths);
    let shown = call(
        &paths,
        "shareFilesFromNative",
        "([Ljava/lang/String;)Z",
        &[],
    );
    if shown {
        done(true);
        Ok(())
    } else {
        Err(AureaError::ElementOperationFailed)
    }
}

fn texts(paths: &[PathBuf]) -> Vec<&str> {
    paths.iter().filter_map(|p| Path::to_str(p)).collect()
}

fn wait(done: impl FnOnce(Vec<PathBuf>) + 'static) -> i64 {
    WAITING.with_borrow_mut(|(next, waiting)| {
        *next += 1;
        waiting.insert(*next, Box::new(done));
        *next
    })
}

fn shown_or_forget(shown: bool, id: i64) -> AureaResult<()> {
    if shown {
        return Ok(());
    }
    WAITING.with_borrow_mut(|(_, waiting)| waiting.remove(&id));
    Err(AureaError::ElementOperationFailed)
}

/// Calls the Activity with `entries` as a Java `String[]`, then `rest`; true when it answers true.
fn call(entries: &[&str], name: &str, signature: &str, rest: &[JValue<'static, 'static>]) -> bool {
    with_activity(|env, activity| {
        let list = string_array(env, entries).ok()?;
        let mut args = vec![JValue::Object(&list)];
        args.extend_from_slice(rest);
        let called = env.call_method(activity, name, signature, &args);
        called.and_then(|v| v.z()).ok()
    })
    .flatten()
    .unwrap_or(false)
}

fn string_array<'local>(env: &mut JNIEnv<'local>, entries: &[&str]) -> Result<JObject<'local>> {
    let len = i32::try_from(entries.len()).unwrap_or(i32::MAX);
    let list = env.new_object_array(len, "java/lang/String", JObject::null())?;
    for (i, entry) in (0..len).zip(entries) {
        let text = env.new_string(entry)?;
        env.set_object_array_element(&list, i, text)?;
    }
    Ok(list.into())
}

#[unsafe(no_mangle)]
extern "system" fn Java_dev_aurea_AureaFiles_nativeFilesDone(
    mut env: JNIEnv,
    _class: JClass,
    id: jlong,
    paths: JObjectArray,
) {
    let mut out = Vec::new();
    let len = env.get_array_length(&paths).unwrap_or(0);
    for i in 0..len {
        let Ok(path) = env.get_object_array_element(&paths, i) else {
            continue;
        };
        if let Ok(path) = env.get_string(&JString::from(path)) {
            out.push(PathBuf::from(String::from(path)));
        }
    }
    let done = WAITING.with_borrow_mut(|(_, waiting)| waiting.remove(&id));
    if let Some(done) = done {
        done(out);
    }
    super::request_frame();
}
