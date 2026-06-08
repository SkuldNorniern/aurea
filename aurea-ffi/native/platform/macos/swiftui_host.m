#import "platform/macos.h"
#import "common/errors.h"
#import <dlfcn.h>
#import <stdint.h>

typedef void* (*create_swiftui_host_fn)(int width, int height);

NGHandle ng_macos_try_create_swiftui_host(int width, int height) {
    if (width <= 0 || height <= 0) return NULL;
    create_swiftui_host_fn fn = (create_swiftui_host_fn)dlsym(RTLD_DEFAULT, "ng_macos_create_swiftui_host_impl");
    if (!fn) return NULL;
    return fn(width, height);
}
