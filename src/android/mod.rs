//! Android: JNI glue for the host Activity, and the app's UI on its `SurfaceView`.

mod host;
mod window;

use std::os::raw::c_void;

use aurea_ffi::{
    ng_android_on_destroy, ng_android_on_pause, ng_android_on_resume, ng_android_on_surface_lost,
    ng_android_on_surface_recreated, ng_android_set_activity,
};
use jni::JNIEnv;
use jni::objects::JObject;
use jni::sys::{JNI_FALSE, JNI_TRUE, JNI_VERSION_1_6, JavaVM, jboolean, jfloat, jint};

use crate::AureaResult;
use crate::ui::{Theme, Ui};

/// Shows `app` in the host Activity: it draws the UI each frame, given the view's logical width and
/// height.
pub fn set_app(
    theme: Theme,
    app: impl FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()> + 'static,
) {
    host::set_app(Box::new(app), theme);
}

/// Switches the theme from the next frame.
pub fn set_theme(theme: Theme) {
    host::set_theme(theme);
}

#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(_vm: JavaVM, _reserved: *mut c_void) -> jint {
    log::info!("Aurea JNI_OnLoad");
    JNI_VERSION_1_6
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeInit(
    env: JNIEnv,
    _thiz: JObject,
    activity: JObject,
) {
    if activity.is_null() {
        return;
    }
    let Ok(vm) = env.get_java_vm() else {
        return;
    };
    let java_vm = vm.get_java_vm_pointer().cast::<c_void>();
    let activity = activity.as_raw().cast::<c_void>();
    unsafe {
        ng_android_set_activity(java_vm, activity);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnPause(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        ng_android_on_pause();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnResume(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        ng_android_on_resume();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnDestroy(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        ng_android_on_destroy();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnSurfaceLost(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        ng_android_on_surface_lost();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnSurfaceRecreated(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        ng_android_on_surface_recreated();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeSurfaceChanged(
    env: JNIEnv,
    _thiz: JObject,
    surface: JObject,
    width: jint,
    height: jint,
    density: jfloat,
) {
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return;
    };
    host::surface_changed(&env, &surface, width, height, density);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeSurfaceDestroyed(
    _env: JNIEnv,
    _thiz: JObject,
) {
    host::surface_destroyed();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeTouch(
    _env: JNIEnv,
    _thiz: JObject,
    action: jint,
    x: jfloat,
    y: jfloat,
) {
    host::touch(action, x, y);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeInsets(
    _env: JNIEnv,
    _thiz: JObject,
    left: jfloat,
    top: jfloat,
    right: jfloat,
    bottom: jfloat,
) {
    host::set_insets(left, top, right, bottom);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeLightTheme(
    _env: JNIEnv,
    _thiz: JObject,
) -> jboolean {
    if host::light_theme() {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

/// Draws a frame; true when the UI wants another.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeFrame(
    _env: JNIEnv,
    _thiz: JObject,
) -> jboolean {
    if host::frame() { JNI_TRUE } else { JNI_FALSE }
}
