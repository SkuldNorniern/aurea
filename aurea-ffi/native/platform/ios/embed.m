#import "../../common/errors.h"
#import "../../common/platform_api.h"

/* SwiftUI hosting is a macOS thing, iOS apps are SwiftUI already. */
NGHandle ng_platform_create_swiftui_host(int width, int height) {
    (void)width;
    (void)height;
    return NULL;
}
