#import "platform/macos.h"
#import "common/errors.h"
#import <dlfcn.h>
#import <stdint.h>

/* SwiftUI-in-Aurea: host element that mounts NSHostingView.
 * The actual NSHostingView is created by Swift (SwiftUIHostImpl).
 * We use dlsym to call the symbol - if the application links a Swift library
 * that provides ng_macos_create_swiftui_host_impl, we use it. Otherwise return NULL. */
typedef void* (*create_swiftui_host_fn)(int width, int height);

NGHandle ng_macos_try_create_swiftui_host(int width, int height) {
    if (width <= 0 || height <= 0) return NULL;
    create_swiftui_host_fn fn = (create_swiftui_host_fn)dlsym(RTLD_DEFAULT, "ng_macos_create_swiftui_host_impl");
    if (!fn) return NULL;
    return fn(width, height);
}
