//! JNI glue for Android: loads native library, forwards Activity to C backend.

use std::os::raw::c_void;

use aurea_ffi::{
    ng_android_on_destroy, ng_android_on_pause, ng_android_on_resume, ng_android_on_surface_lost,
    ng_android_on_surface_recreated, ng_android_set_activity,
};
use jni::JNIEnv;
use jni::objects::JObject;
use jni::sys::{JNI_VERSION_1_6, JavaVM, jint};

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
