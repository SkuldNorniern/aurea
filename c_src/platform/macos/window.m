#import "window.h"
#import "../../common/errors.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>

// Window delegate to handle window close events and scale factor changes
#import "window.h"

// Forward declaration for lifecycle callback
extern void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);

@interface WindowDelegate : NSObject <NSWindowDelegate>
@property (nonatomic, assign) void* windowHandle;
@property (nonatomic, assign) ScaleFactorCallback scaleFactorCallback;
@property (nonatomic, assign) BOOL lifecycleCallbackEnabled;
@end

@implementation WindowDelegate
- (BOOL)windowShouldClose:(NSWindow*)sender {
    // Invoke lifecycle callback if enabled
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 5); // WindowWillClose = 5
    }
    // Terminate the application when the window closes
    [NSApp terminate:nil];
    return YES;
}

- (void)windowDidMiniaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 6); // WindowMinimized = 6
    }
}

- (void)windowDidDeminiaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 7); // WindowRestored = 7
    }
}

- (void)windowDidChangeScreen:(NSNotification*)notification {
    if (self.scaleFactorCallback && self.windowHandle) {
        NSWindow* window = (NSWindow*)self.windowHandle;
        NSScreen* screen = [window screen];
        if (screen) {
            float scale = (float)[screen backingScaleFactor];
            self.scaleFactorCallback(self.windowHandle, scale);
        }
    }
}

- (void)windowDidChangeBackingProperties:(NSNotification*)notification {
    if (self.scaleFactorCallback && self.windowHandle) {
        NSWindow* window = (NSWindow*)self.windowHandle;
        NSScreen* screen = [window screen];
        if (screen) {
            float scale = (float)[screen backingScaleFactor];
            self.scaleFactorCallback(self.windowHandle, scale);
        }
    }
}
@end

static NSMutableDictionary* windowDelegates = nil;

NGHandle ng_macos_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    if (!windowDelegates) {
        windowDelegates = [[NSMutableDictionary alloc] init];
    }
    
    NSRect frame = NSMakeRect(0, 0, width, height);
    NSWindow* window = [[NSWindow alloc] 
        initWithContentRect:frame
        styleMask:NSWindowStyleMaskTitled |
                 NSWindowStyleMaskClosable |
                 NSWindowStyleMaskMiniaturizable |
                 NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered
        defer:NO];
    
    WindowDelegate* delegate = [[WindowDelegate alloc] init];
    delegate.windowHandle = (__bridge void*)window;
    [window setDelegate:delegate];
    
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)window];
    [windowDelegates setObject:delegate forKey:windowValue];
    
    [window setTitle:ng_macos_to_nsstring(title)];
    [window center];
    [window makeKeyAndOrderFront:nil];
    
    // Create a content view that will hold our elements
    NSView* contentView = [[NSView alloc] initWithFrame:frame];
    [window setContentView:contentView];
    
    return (__bridge_retained void*)window;
}

float ng_macos_get_scale_factor(NGHandle window) {
    if (!window) return 1.0f;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSScreen* screen = [nsWindow screen];
    if (screen) {
        return (float)[screen backingScaleFactor];
    }
    return 1.0f;
}

void ng_macos_destroy_window(NGHandle handle) {
    if (!handle) return;
    NSWindow* window = (__bridge_transfer NSWindow*)handle;
    if (windowDelegates) {
        NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)window];
        [windowDelegates removeObjectForKey:windowValue];
    }
    [window close];
}

void ng_macos_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window || !windowDelegates) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)nsWindow];
    WindowDelegate* delegate = [windowDelegates objectForKey:windowValue];
    if (delegate) {
        delegate.scaleFactorCallback = callback;
    }
}

void ng_macos_window_set_lifecycle_callback(NGHandle window) {
    if (!window || !windowDelegates) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)nsWindow];
    WindowDelegate* delegate = [windowDelegates objectForKey:windowValue];
    if (delegate) {
        delegate.lifecycleCallbackEnabled = YES;
    }
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