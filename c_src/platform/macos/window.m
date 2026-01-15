#import "window.h"
#import "../../common/errors.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>

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
    return YES; // Allow the window to close
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

- (void)windowDidMove:(NSNotification *)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 11); // WindowMoved = 11
    }
}

- (void)windowDidResize:(NSNotification *)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 12); // WindowResized = 12
    }
}
@end

static NSMutableDictionary* windowDelegates = nil;

NGHandle ng_macos_create_window(const char* title, int width, int height) {
    return ng_macos_create_window_with_type(title, width, height, 0);
}

NGHandle ng_macos_create_window_with_type(const char* title, int width, int height, int window_type) {
    if (!title) {
        return NULL;
    }
    
    if (width <= 0 || height <= 0) {
        return NULL;
    }
    
    if (!windowDelegates) {
        windowDelegates = [[NSMutableDictionary alloc] init];
    }
    
    NSRect frame = NSMakeRect(0, 0, width, height);
    NSWindowStyleMask styleMask = 0;
    NSWindowLevel windowLevel = NSNormalWindowLevel;
    BOOL isSheet = NO;
    
    switch (window_type) {
        case 0: // Normal
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskMiniaturizable |
                       NSWindowStyleMaskResizable;
            break;
        case 1: // Popup
            styleMask = NSWindowStyleMaskBorderless;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 2: // Tool
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskUtilityWindow |
                       NSWindowStyleMaskResizable;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 3: // Utility
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskUtilityWindow |
                       NSWindowStyleMaskResizable;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 4: // Sheet
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable;
            isSheet = YES;
            break;
        case 5: // Dialog
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskResizable;
            break;
        default:
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskMiniaturizable |
                       NSWindowStyleMaskResizable;
            break;
    }
    
    NSWindow* window = [[NSWindow alloc] 
        initWithContentRect:frame
        styleMask:styleMask
        backing:NSBackingStoreBuffered
        defer:NO];
    
    [window setLevel:windowLevel];
    [window setReleasedWhenClosed:NO]; // Crucial for show/hide behavior
    
    WindowDelegate* delegate = [[WindowDelegate alloc] init];
    delegate.windowHandle = (__bridge void*)window;
    [window setDelegate:delegate];
    
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)window];
    [windowDelegates setObject:delegate forKey:windowValue];
    
    [window setTitle:ng_macos_to_nsstring(title)];
    
    if (!isSheet) {
        [window center];
    }
    [window makeKeyAndOrderFront:nil];
    
    NSView* contentView = [[NSView alloc] initWithFrame:frame];
    [contentView setWantsLayer:YES];
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

void ng_macos_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSString* nsTitle = [NSString stringWithUTF8String:title];
    [nsWindow setTitle:nsTitle];
}

void ng_macos_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect frame = [nsWindow frame];
    frame.size.width = width;
    frame.size.height = height;
    [nsWindow setFrame:frame display:YES];
}

void ng_macos_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect frame = [nsWindow frame];
    *width = (int)frame.size.width;
    *height = (int)frame.size.height;
}

void ng_macos_window_set_position(NGHandle window, int x, int y) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSScreen* screen = [[NSScreen screens] objectAtIndex:0];
    NSRect screenFrame = [screen frame];
    
    // Convert top-left (Aurea) to bottom-left (Cocoa)
    NSPoint point = NSMakePoint(x, screenFrame.size.height - y);
    [nsWindow setFrameTopLeftPoint:point];
}

void ng_macos_window_get_position(NGHandle window, int* x, int* y) {
    if (!window || !x || !y) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect frame = [nsWindow frame];
    NSScreen* screen = [[NSScreen screens] objectAtIndex:0];
    NSRect screenFrame = [screen frame];
    
    *x = (int)frame.origin.x;
    *y = (int)(screenFrame.size.height - (frame.origin.y + frame.size.height));
}

void ng_macos_window_request_close(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    
    // performClose: only works if the window has a close button in styleMask.
    // For borderless windows, we must trigger the logic manually.
    if ([nsWindow styleMask] & NSWindowStyleMaskClosable) {
        [nsWindow performClose:nil];
    } else {
        id<NSWindowDelegate> delegate = [nsWindow delegate];
        if ([delegate respondsToSelector:@selector(windowShouldClose:)]) {
            [delegate windowShouldClose:nsWindow];
        }
    }
}

int ng_macos_window_is_focused(NGHandle window) {
    if (!window) return 0;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    return [nsWindow isKeyWindow] ? 1 : 0;
}

void ng_macos_window_show(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [nsWindow makeKeyAndOrderFront:nil];
}

void ng_macos_window_hide(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [nsWindow orderOut:nil];
}

int ng_macos_window_is_visible(NGHandle window) {
    if (!window) return 0;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    return [nsWindow isVisible] ? 1 : 0;
}

NGHandle ng_macos_window_get_content_view(NGHandle window) {
    if (!window) return NULL;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* contentView = [nsWindow contentView];
    return (__bridge void*)contentView;
} 