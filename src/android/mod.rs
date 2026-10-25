//! Android: the app's UI filling an `AureaActivity`, drawn on the GPU with the `zengpu` feature
//! and on the CPU otherwise, or when the GPU cannot start. Fingers come in as the mouse, two-finger
//! scrolls and pinches as the wheel and [`WindowEvent::Magnify`], a pen also as
//! [`WindowEvent::Pen`] with its pressure and lean; see `crate::touch`.
//!
//! The Java side is `dev.aurea.AureaActivity`, in the `aurea` Gradle module next to the sample in
//! `mobile/android/`. An app subclasses it, names its native library, and defines `aurea_main`,
//! which calls [`run`]:
//!
//! ```rust,ignore
//! #[unsafe(no_mangle)]
//! pub extern "C" fn aurea_main() -> std::ffi::c_int {
//!     aurea::android::run(Theme::dark(), |ui, width, height| draw(ui, width, height));
//!     0
//! }
//! ```

mod host;
mod logcat;
mod window;

use std::ffi::{c_int, c_void};
#[cfg(feature = "zengpu")]
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use aurea_ffi::{
    ng_android_on_destroy, ng_android_on_pause, ng_android_on_resume, ng_android_on_surface_lost,
    ng_android_on_surface_recreated, ng_android_set_activity,
};
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{JNI_FALSE, JNI_TRUE, JNI_VERSION_1_6, jboolean, jdouble, jfloat, jint};
use jni::{JNIEnv, JavaVM};

#[cfg(feature = "zengpu")]
use crate::gpu_ready;
use crate::ui::{Theme, Ui};
use crate::{AureaResult, WindowEvent};
#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuContext;

unsafe extern "C" {
    /// The app's entry: it calls [`run`]. `AureaActivity` calls it once, when it is first made.
    fn aurea_main() -> c_int;
}

/// The Activity, for asking it for frames from any thread.
static ACTIVITY: Mutex<Option<(JavaVM, GlobalRef)>> = Mutex::new(None);

/// Runs `app` as the app's UI: it draws the UI each frame, given the safe area's logical width and
/// height. Call it from `aurea_main`; it returns, and the Activity drives frames from then on.
pub fn run(theme: Theme, app: impl FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()> + 'static) {
    host::set_app(Box::new(app), theme);
}

/// Calls `listener` with each finger, as mouse events, and each pen reading, as
/// [`WindowEvent::Pen`] with pressure and tilt, the way a desktop window reports a pen. It also
/// hears the app's lifecycle: focus, [`WindowEvent::Minimized`] on going to the background (save
/// there; Android may stop the app after), and [`WindowEvent::MemoryWarning`].
/// Every listener added hears every event, in the order they were added.
pub fn on_event(listener: impl FnMut(&WindowEvent) + 'static) {
    host::add_listener(Box::new(listener));
}

/// Asks for a frame from any thread, such as when background work finishes. The UI's own
/// animations and input already draw frames; this is for what they cannot see.
pub fn request_frame() {
    call_activity("requestFrameFromNative", "()V", &[]);
}

/// Shows the on-screen keyboard, or hides it, from any thread, such as when a text field takes or
/// loses focus. What it types comes as [`WindowEvent::TextInput`], what an input method such as
/// Korean or Japanese is still composing as [`WindowEvent::ImePreedit`], and keys such as delete
/// and enter as [`WindowEvent::KeyInput`].
pub fn show_keyboard(show: bool) {
    call_activity(
        "showKeyboardFromNative",
        "(Z)V",
        &[JValue::Bool(u8::from(show))],
    );
}

/// Calls a method of the Activity from any thread.
fn call_activity(name: &str, signature: &str, args: &[JValue<'_, '_>]) {
    let Ok(activity) = ACTIVITY.lock() else {
        return;
    };
    let Some((vm, activity)) = activity.as_ref() else {
        return;
    };
    // detached again when done, so short-lived threads (wakes) leave nothing behind.
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    if env.call_method(activity, name, signature, args).is_err() {
        let _ = env.exception_clear();
    }
}

/// The screen's pixels per logical pixel: about 2.6 on a typical phone, 1 before the first surface.
pub fn scale() -> f32 {
    host::scale()
}

/// The safe area's insets, left, top, right and bottom, in logical pixels. The app draws only
/// inside them: the UI's origin is the safe area's top left, and the theme's background fills the
/// rest of the screen.
pub fn safe_area() -> [f32; 4] {
    host::safe_area()
}

/// Whether to draw on the GPU, through ZenGPU's Vulkan backend; call before [`run`]. On by default
/// with the `zengpu` feature, which it needs; the CPU draws when it is off, or when the GPU cannot
/// start.
pub fn use_gpu(on: bool) {
    host::GPU_WANTED.store(on, Ordering::Relaxed);
}

/// The GPU the host draws with, once it has a surface on it, for the app's own textures and
/// renderers to share.
#[cfg(feature = "zengpu")]
pub fn gpu_context() -> Option<Arc<ZenGpuContext>> {
    host::gpu_context()
}

/// Calls `ready` with the host's GPU once it is up, or now if it is, for the app's own textures and
/// renderers; it is not called while the host draws on the CPU.
#[cfg(feature = "zengpu")]
pub fn on_gpu_ready(ready: impl FnOnce(Arc<ZenGpuContext>) + 'static) {
    gpu_ready::on_ready(ready);
}

/// Switches the theme from the next frame.
pub fn set_theme(theme: Theme) {
    host::set_theme(theme);
}

#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(_vm: *mut c_void, _reserved: *mut c_void) -> jint {
    logcat::install();
    JNI_VERSION_1_6
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeCreate(
    env: JNIEnv,
    _class: JClass,
    activity: JObject,
) {
    if activity.is_null() {
        return;
    }
    let Ok(vm) = env.get_java_vm() else {
        return;
    };
    let java_vm = vm.get_java_vm_pointer().cast::<c_void>();
    unsafe { ng_android_set_activity(java_vm, activity.as_raw().cast::<c_void>()) };
    let first = match (ACTIVITY.lock(), env.new_global_ref(&activity)) {
        (Ok(mut slot), Ok(global)) => slot.replace((vm, global)).is_none(),
        _ => false,
    };
    // the Activity is made again on rotation; the app keeps running.
    if first {
        unsafe { aurea_main() };
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    finishing: jboolean,
) {
    unsafe { ng_android_on_destroy() };
    // a finished Activity is let go; one made for a rotation replaces it in nativeCreate.
    if finishing != JNI_FALSE
        && let Ok(mut activity) = ACTIVITY.lock()
    {
        *activity = None;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeLifecycle(
    _env: JNIEnv,
    _class: JClass,
    state: jint,
) {
    match state {
        0 => unsafe { ng_android_on_resume() },
        1 => unsafe { ng_android_on_pause() },
        _ => {}
    }
    host::lifecycle(state);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeSurfaceChanged(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
    width: jint,
    height: jint,
    density: jfloat,
) {
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return;
    };
    unsafe { ng_android_on_surface_recreated() };
    host::surface_changed(&env, &surface, width, height, density);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeSurfaceDestroyed(
    _env: JNIEnv,
    _class: JClass,
) {
    host::surface_destroyed();
    unsafe { ng_android_on_surface_lost() };
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativePointer(
    _env: JNIEnv,
    _class: JClass,
    phase: jint,
    id: jint,
    tool: jint,
    x: jfloat,
    y: jfloat,
    pressure: jfloat,
    tilt: jfloat,
    orientation: jfloat,
    barrel: jboolean,
    age: jdouble,
) {
    host::pointer(
        phase,
        id,
        tool,
        (x, y),
        pressure,
        (tilt, orientation),
        barrel != JNI_FALSE,
        age,
    );
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeInsets(
    _env: JNIEnv,
    _class: JClass,
    left: jfloat,
    top: jfloat,
    right: jfloat,
    bottom: jfloat,
) {
    host::set_insets(left, top, right, bottom);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeLightTheme(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if host::light_theme() {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeKey(
    _env: JNIEnv,
    _class: JClass,
    code: jint,
    pressed: jboolean,
    meta: jint,
) {
    host::key(code, pressed != JNI_FALSE, meta);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeText(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
) {
    if let Ok(text) = env.get_string(&text) {
        host::text(text.into());
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativePreedit(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
    cursor: jint,
) {
    if let Ok(text) = env.get_string(&text) {
        host::preedit(text.into(), cursor);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_aurea_AureaActivity_nativeFrame(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if host::frame() { JNI_TRUE } else { JNI_FALSE }
}
