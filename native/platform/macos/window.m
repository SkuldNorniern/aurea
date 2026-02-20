#import "window.h"
#import "common/errors.h"
#import "common/input.h"
#import "common/rust_callbacks.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>

static int ng_macos_get_cursor_grab_mode(void* windowHandle);

@interface WindowDelegate : NSObject <NSWindowDelegate>
@property (nonatomic, assign) void* windowHandle;
@property (nonatomic, assign) ScaleFactorCallback scaleFactorCallback;
@property (nonatomic, assign) BOOL lifecycleCallbackEnabled;
@end

static unsigned int ng_macos_modifiers(NSEvent* event) {
    NSEventModifierFlags flags = [event modifierFlags];
    unsigned int mods = 0;
    if (flags & NSEventModifierFlagShift) {
        mods |= NG_MOD_SHIFT;
    }
    if (flags & NSEventModifierFlagControl) {
        mods |= NG_MOD_CTRL;
    }
    if (flags & NSEventModifierFlagOption) {
        mods |= NG_MOD_ALT;
    }
    if (flags & NSEventModifierFlagCommand) {
        mods |= NG_MOD_META;
    }
    return mods;
}

static BOOL ng_macos_consume_raw_suppress(void* windowHandle);

static unsigned int ng_macos_keycode_from_event(unsigned short keycode) {
    switch (keycode) {
        case 0: return NG_KEY_A;
        case 1: return NG_KEY_S;
        case 2: return NG_KEY_D;
        case 3: return NG_KEY_F;
        case 4: return NG_KEY_H;
        case 5: return NG_KEY_G;
        case 6: return NG_KEY_Z;
        case 7: return NG_KEY_X;
        case 8: return NG_KEY_C;
        case 9: return NG_KEY_V;
        case 11: return NG_KEY_B;
        case 12: return NG_KEY_Q;
        case 13: return NG_KEY_W;
        case 14: return NG_KEY_E;
        case 15: return NG_KEY_R;
        case 16: return NG_KEY_Y;
        case 17: return NG_KEY_T;
        case 18: return NG_KEY_1;
        case 19: return NG_KEY_2;
        case 20: return NG_KEY_3;
        case 21: return NG_KEY_4;
        case 22: return NG_KEY_6;
        case 23: return NG_KEY_5;
        case 25: return NG_KEY_9;
        case 26: return NG_KEY_7;
        case 28: return NG_KEY_8;
        case 29: return NG_KEY_0;
        case 31: return NG_KEY_O;
        case 32: return NG_KEY_U;
        case 34: return NG_KEY_I;
        case 35: return NG_KEY_P;
        case 37: return NG_KEY_L;
        case 38: return NG_KEY_J;
        case 40: return NG_KEY_K;
        case 45: return NG_KEY_N;
        case 46: return NG_KEY_M;
        case 49: return NG_KEY_SPACE;
        case 36: return NG_KEY_ENTER;
        case 48: return NG_KEY_TAB;
        case 51: return NG_KEY_BACKSPACE;
        case 117: return NG_KEY_DELETE;
        case 53: return NG_KEY_ESCAPE;
        case 115: return NG_KEY_HOME;
        case 119: return NG_KEY_END;
        case 116: return NG_KEY_PAGE_UP;
        case 121: return NG_KEY_PAGE_DOWN;
        case 123: return NG_KEY_LEFT;
        case 124: return NG_KEY_RIGHT;
        case 125: return NG_KEY_DOWN;
        case 126: return NG_KEY_UP;
        case 122: return NG_KEY_F1;
        case 120: return NG_KEY_F2;
        case 99: return NG_KEY_F3;
        case 118: return NG_KEY_F4;
        case 96: return NG_KEY_F5;
        case 97: return NG_KEY_F6;
        case 98: return NG_KEY_F7;
        case 100: return NG_KEY_F8;
        case 101: return NG_KEY_F9;
        case 109: return NG_KEY_F10;
        case 103: return NG_KEY_F11;
        case 111: return NG_KEY_F12;
        case 56:
        case 60:
            return NG_KEY_SHIFT;
        case 59:
        case 62:
            return NG_KEY_CONTROL;
        case 58:
        case 61:
            return NG_KEY_ALT;
        case 55:
        case 54:
            return NG_KEY_META;
        case 114:
            return NG_KEY_INSERT;
        default:
            return NG_KEY_UNKNOWN;
    }
}

@interface AureaContentView : NSView {
    NSTrackingArea* trackingArea;
}
@property (nonatomic, assign) void* windowHandle;
@end

@implementation AureaContentView
- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent*)event {
    (void)event;
    return YES;
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if ([self window]) {
        [[self window] makeFirstResponder:self];
    }
    [self updateTrackingAreas];
}

- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    if (trackingArea) {
        [self removeTrackingArea:trackingArea];
        trackingArea = nil;
    }
    NSTrackingAreaOptions options = NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved |
        NSTrackingActiveAlways | NSTrackingInVisibleRect;
    trackingArea = [[NSTrackingArea alloc] initWithRect:[self bounds] options:options owner:self userInfo:nil];
    [self addTrackingArea:trackingArea];
}

- (void)mouseEntered:(NSEvent*)event {
    if (self.windowHandle) {
        ng_invoke_cursor_entered(self.windowHandle, 1);
    }
}

- (void)mouseExited:(NSEvent*)event {
    if (self.windowHandle) {
        ng_invoke_cursor_entered(self.windowHandle, 0);
    }
}

- (void)mouseMoved:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint location = [self convertPoint:[event locationInWindow] fromView:nil];
    ng_invoke_mouse_move(self.windowHandle, location.x, location.y);

    int mode = ng_macos_get_cursor_grab_mode(self.windowHandle);
    if (mode == 2) {
        if (ng_macos_consume_raw_suppress(self.windowHandle)) {
            return;
        }
        ng_invoke_raw_mouse_motion(self.windowHandle, (double)[event deltaX], (double)[event deltaY]);
    }
}

- (void)mouseDragged:(NSEvent*)event {
    [self mouseMoved:event];
}

- (void)rightMouseDragged:(NSEvent*)event {
    [self mouseMoved:event];
}

- (void)otherMouseDragged:(NSEvent*)event {
    [self mouseMoved:event];
}

- (void)mouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_button(self.windowHandle, 0, 1, ng_macos_modifiers(event));
}

- (void)mouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_button(self.windowHandle, 0, 0, ng_macos_modifiers(event));
}

- (void)rightMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_button(self.windowHandle, 1, 1, ng_macos_modifiers(event));
}

- (void)rightMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_button(self.windowHandle, 1, 0, ng_macos_modifiers(event));
}

- (void)otherMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    int button = (int)[event buttonNumber];
    ng_invoke_mouse_button(self.windowHandle, button, 1, ng_macos_modifiers(event));
}

- (void)otherMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    int button = (int)[event buttonNumber];
    ng_invoke_mouse_button(self.windowHandle, button, 0, ng_macos_modifiers(event));
}

- (void)scrollWheel:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_wheel(
        self.windowHandle,
        (double)[event scrollingDeltaX],
        (double)[event scrollingDeltaY],
        ng_macos_modifiers(event));
}

- (void)keyDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
    ng_invoke_key_event(self.windowHandle, keycode, 1, ng_macos_modifiers(event));

    NSString* chars = [event characters];
    if (chars && [chars length] > 0) {
        const char* utf8 = [chars UTF8String];
        if (utf8) {
            ng_invoke_text_input(self.windowHandle, utf8);
        }
    }
}

- (void)keyUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
    ng_invoke_key_event(self.windowHandle, keycode, 0, ng_macos_modifiers(event));
}
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
        ng_invoke_lifecycle_callback(self.windowHandle, 9); // SurfaceLost = 9
    }
}

- (void)windowDidDeminiaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 7); // WindowRestored = 7
        ng_invoke_lifecycle_callback(self.windowHandle, 10); // SurfaceRecreated = 10
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

- (void)windowDidBecomeKey:(NSNotification*)notification {
    if (self.windowHandle) {
        ng_invoke_focus_changed(self.windowHandle, 1);
    }
}

- (void)windowDidResignKey:(NSNotification*)notification {
    if (self.windowHandle) {
        ng_invoke_focus_changed(self.windowHandle, 0);
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
static NSMutableDictionary* windowCursorGrabModes = nil;
static NSMutableDictionary* windowRawSuppress = nil;

static int ng_macos_get_cursor_grab_mode(void* windowHandle) {
    if (!windowCursorGrabModes || !windowHandle) {
        return 0;
    }
    NSWindow* window = (__bridge NSWindow*)windowHandle;
    NSValue* key = [NSValue valueWithPointer:(__bridge const void*)window];
    NSNumber* mode = [windowCursorGrabModes objectForKey:key];
    if (!mode) {
        return 0;
    }
    return (int)[mode integerValue];
}

static BOOL ng_macos_consume_raw_suppress(void* windowHandle) {
    if (!windowRawSuppress || !windowHandle) {
        return NO;
    }
    NSWindow* window = (__bridge NSWindow*)windowHandle;
    NSValue* key = [NSValue valueWithPointer:(__bridge const void*)window];
    NSNumber* value = [windowRawSuppress objectForKey:key];
    if (!value) {
        return NO;
    }
    BOOL suppress = [value boolValue];
    if (suppress) {
        [windowRawSuppress setObject:@(NO) forKey:key];
    }
    return suppress;
}

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

    [window setAcceptsMouseMovedEvents:YES];
    AureaContentView* contentView = [[AureaContentView alloc] initWithFrame:frame];
    contentView.windowHandle = (__bridge void*)window;
    [contentView setWantsLayer:YES];
    [window setContentView:contentView];
    [window makeFirstResponder:contentView];
    [contentView updateTrackingAreas];

    [window makeKeyAndOrderFront:nil];
    
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
    if (windowCursorGrabModes) {
        NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)window];
        [windowCursorGrabModes removeObjectForKey:windowValue];
    }
    if (windowRawSuppress) {
        NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)window];
        [windowRawSuppress removeObjectForKey:windowValue];
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

int ng_macos_window_set_cursor_visible(NGHandle window, int visible) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    if (visible) {
        CGDisplayShowCursor(kCGDirectMainDisplay);
    } else {
        CGDisplayHideCursor(kCGDirectMainDisplay);
    }
    return NG_SUCCESS;
}

int ng_macos_window_set_cursor_grab(NGHandle window, int mode) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;

    if (!windowCursorGrabModes) {
        windowCursorGrabModes = [[NSMutableDictionary alloc] init];
    }
    if (!windowRawSuppress) {
        windowRawSuppress = [[NSMutableDictionary alloc] init];
    }

    NSValue* key = [NSValue valueWithPointer:(__bridge const void*)nsWindow];
    [windowCursorGrabModes setObject:@(mode) forKey:key];

    if (mode == 2) {
        [windowRawSuppress setObject:@(YES) forKey:key];
        CGAssociateMouseAndMouseCursorPosition(false);
        NSRect frame = [nsWindow frame];
        CGPoint center = CGPointMake(NSMidX(frame), NSMidY(frame));
        CGWarpMouseCursorPosition(center);
        CGAssociateMouseAndMouseCursorPosition(false);
    } else {
        CGAssociateMouseAndMouseCursorPosition(true);
        [windowRawSuppress setObject:@(NO) forKey:key];
    }

    return NG_SUCCESS;
}

void ng_macos_window_show(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [NSApp activateIgnoringOtherApps:YES];
    [nsWindow makeKeyAndOrderFront:nil];
    NSView* contentView = [nsWindow contentView];
    if (contentView) {
        [nsWindow makeFirstResponder:contentView];
    }
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
