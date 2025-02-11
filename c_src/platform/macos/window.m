#import "window.h"
#import "../../common/errors.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
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
    
    // Create a content view that will hold our elements
    NSView* contentView = [[NSView alloc] initWithFrame:frame];
    [window setContentView:contentView];
    
    return (__bridge_retained void*)window;
}

void ng_macos_destroy_window(NGHandle handle) {
    if (!handle) return;
    NSWindow* window = (__bridge_transfer NSWindow*)handle;
    [window close];
}

int ng_macos_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!window_handle || !content_handle) return NG_ERROR_INVALID_HANDLE;
    
    NSWindow* window = (__bridge NSWindow*)window_handle;
    NSView* contentView = (__bridge NSView*)content_handle;
    
    // Get the window's content view
    NSView* mainContentView = [window contentView];
    
    // Set the content view's frame to match the window's content area
    [contentView setFrame:[mainContentView bounds]];
    
    // Add the content as a subview
    [mainContentView addSubview:contentView];
    
    // Set up Auto Layout constraints
    contentView.translatesAutoresizingMaskIntoConstraints = NO;
    
    // Pin the content view to all sides of the window's content view
    NSArray* constraints = @[
        [contentView.topAnchor constraintEqualToAnchor:mainContentView.topAnchor],
        [contentView.leadingAnchor constraintEqualToAnchor:mainContentView.leadingAnchor],
        [contentView.trailingAnchor constraintEqualToAnchor:mainContentView.trailingAnchor],
        [contentView.bottomAnchor constraintEqualToAnchor:mainContentView.bottomAnchor]
    ];
    
    [NSLayoutConstraint activateConstraints:constraints];
    
    return NG_SUCCESS;
} 