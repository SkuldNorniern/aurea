//! JNI glue for Android: loads native library, forwards Activity to C backend.
//! Compiled only when target_os = "android".

use jni::objects::JObject;
use jni::JNIEnv;
use std::os::raw::c_void;

const JNI_VERSION: jni::sys::jint = jni::sys::JNI_VERSION_1_6;

#[no_mangle]
pub extern "system" fn JNI_OnLoad(
    vm: jni::sys::JavaVM,
    _reserved: *mut c_void,
) -> jni::sys::jint {
    log::info!("Aurea JNI_OnLoad");
    JNI_VERSION
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeInit(
    env: JNIEnv,
    _thiz: JObject,
    activity: JObject,
) {
    if activity.is_null() {
        return;
    }
    let vm = match env.get_java_vm() {
        Ok(v) => v,
        Err(_) => return,
    };
    let java_vm_ptr = vm.get_java_vm_pointer();
    let activity_raw = activity.as_raw() as *mut c_void;
    unsafe {
        aurea_ffi::ng_android_set_activity(java_vm_ptr as *mut c_void, activity_raw);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnPause(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        aurea_ffi::ng_android_on_pause();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnResume(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        aurea_ffi::ng_android_on_resume();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnDestroy(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        aurea_ffi::ng_android_on_destroy();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnSurfaceLost(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        aurea_ffi::ng_android_on_surface_lost();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurea_smoke_MainActivity_nativeOnSurfaceRecreated(
    _env: JNIEnv,
    _thiz: JObject,
) {
    unsafe {
        aurea_ffi::ng_android_on_surface_recreated();
    }
}
