#import "window.h"
#import "../../common/errors.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_window(const char* title, int width, int height) {
    NSRect frame = NSMakeRect(0, 0, width, height);
    NSWindow* window = [[NSWindow alloc]
        initWithContentRect:frame
        styleMask:NSWindowStyleMaskTitled |
                 NSWindowStyleMaskClosable |
                 NSWindowStyleMaskMiniaturizable |
                 NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered
        defer:NO];

    [window setTitle:ng_macos_to_nsstring(title)];
    [window center];
    [window makeKeyAndOrderFront:nil];
    
    return (__bridge_retained void*)window;
}

void ng_macos_destroy_window(NGHandle handle) {
    if (!handle) return;
    NSWindow* window = (__bridge_transfer NSWindow*)handle;
    [window close];
} 