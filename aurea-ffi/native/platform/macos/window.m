#import "window.h"
#import "common/errors.h"
#import "common/input.h"
#import "common/rust_callbacks.h"
#import "common/shortcuts.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>
#include <string.h>

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
        case 27: return NG_KEY_MINUS;
        case 24: return NG_KEY_EQUALS;
        case 33: return NG_KEY_LEFT_BRACKET;
        case 30: return NG_KEY_RIGHT_BRACKET;
        case 42: return NG_KEY_BACKSLASH;
        case 41: return NG_KEY_SEMICOLON;
        case 39: return NG_KEY_APOSTROPHE;
        case 50: return NG_KEY_GRAVE;
        case 43: return NG_KEY_COMMA;
        case 47: return NG_KEY_PERIOD;
        case 44: return NG_KEY_SLASH;
        default:
            return NG_KEY_UNKNOWN;
    }
}

@interface AureaContentView : NSView <NSTextInputClient> {
    NSTrackingArea* trackingArea;
    int buttonsHeld;
    BOOL exitHeld;
    unsigned int modifiersDown;
    NSString* markedText;
    NSRect imeArea;
}
@property (nonatomic, assign) void* windowHandle;
@property (nonatomic, strong) NSCursor* cursor;
- (void)setImeArea:(NSRect)area;
@end

@implementation AureaContentView
- (void)resetCursorRects {
    [self addCursorRect:[self bounds] cursor:(self.cursor ?: [NSCursor arrowCursor])];
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent*)event {
    (void)event;
    return YES;
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    [self updateTrackingAreas];
}

- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    if (trackingArea) {
        [self removeTrackingArea:trackingArea];
        trackingArea = nil;
    }
    NSTrackingAreaOptions options = NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved |
        NSTrackingActiveInActiveApp | NSTrackingInVisibleRect;
    trackingArea = [[NSTrackingArea alloc] initWithRect:[self bounds] options:options owner:self userInfo:nil];
    [self addTrackingArea:trackingArea];
}

- (void)mouseEntered:(NSEvent*)event {
    if (exitHeld) {
        exitHeld = NO;
        return;
    }
    if (self.windowHandle) {
        ng_invoke_cursor_entered(self.windowHandle, 1);
    }
}

- (void)mouseExited:(NSEvent*)event {
    if (buttonsHeld > 0) {
        exitHeld = YES;
        return;
    }
    if (self.windowHandle) {
        ng_invoke_cursor_entered(self.windowHandle, 0);
    }
}

- (void)buttonWentDown {
    buttonsHeld++;
}

- (void)buttonWentUp {
    if (buttonsHeld > 0) buttonsHeld--;
    if (buttonsHeld == 0 && exitHeld) {
        exitHeld = NO;
        if (self.windowHandle) {
            ng_invoke_cursor_entered(self.windowHandle, 0);
        }
    }
}

- (void)mouseMoved:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint location = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    unsigned int buttons = (unsigned int)([NSEvent pressedMouseButtons] & 0xFF);
    ng_invoke_mouse_move(self.windowHandle, location.x, h - location.y, buttons, ng_macos_modifiers(event));

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
    [[self window] makeFirstResponder:self];
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 0, 1, ng_macos_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    [self buttonWentDown];
}

- (void)mouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 0, 0, ng_macos_modifiers(event), p.x, h - p.y, 1);
    [self buttonWentUp];
}

- (void)rightMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 1, 1, ng_macos_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    [self buttonWentDown];
}

- (void)rightMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 1, 0, ng_macos_modifiers(event), p.x, h - p.y, 1);
    [self buttonWentUp];
}

- (void)otherMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    int button = (int)[event buttonNumber];
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, button, 1, ng_macos_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    [self buttonWentDown];
}

- (void)otherMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    int button = (int)[event buttonNumber];
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, button, 0, ng_macos_modifiers(event), p.x, h - p.y, 1);
    [self buttonWentUp];
}

- (void)scrollWheel:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_invoke_mouse_wheel(
        self.windowHandle,
        -(double)[event scrollingDeltaX],
        (double)[event scrollingDeltaY],
        ng_macos_modifiers(event));
}

- (void)keyDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    if (![self hasMarkedText]) {
        unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
        ng_invoke_key_event(self.windowHandle, keycode, 1, ng_macos_modifiers(event));
    }
    [self interpretKeyEvents:@[event]];
}

- (void)insertText:(id)string replacementRange:(NSRange)replacementRange {
    (void)replacementRange;
    NSString* text = [string isKindOfClass:[NSAttributedString class]] ? [string string] : string;
    if ([self hasMarkedText]) {
        markedText = nil;
        if (self.windowHandle) ng_invoke_ime_preedit(self.windowHandle, "", -1);
    }
    if (!self.windowHandle || !text || [text length] == 0) return;

    unichar first = [text characterAtIndex:0];
    BOOL isControl = (first < 0x0020 || first == 0x007F);
    BOOL isFnKeyChar = (first >= 0xF700 && first <= 0xF8FF);
    if (isControl || isFnKeyChar) return;

    const char* utf8 = [text UTF8String];
    if (utf8) {
        ng_invoke_text_input(self.windowHandle, utf8);
    }
}

- (void)setMarkedText:(id)string selectedRange:(NSRange)selectedRange replacementRange:(NSRange)replacementRange {
    (void)replacementRange;
    NSString* text = [string isKindOfClass:[NSAttributedString class]] ? [string string] : string;
    markedText = (text && [text length] > 0) ? [text copy] : nil;
    if (!self.windowHandle) return;

    int cursor = -1;
    if (markedText && selectedRange.location != NSNotFound && selectedRange.location <= [markedText length]) {
        NSString* head = [markedText substringToIndex:selectedRange.location];
        cursor = (int)[head lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
    }
    const char* utf8 = markedText ? [markedText UTF8String] : "";
    ng_invoke_ime_preedit(self.windowHandle, utf8 ? utf8 : "", cursor);
}

- (void)unmarkText {
    if (![self hasMarkedText]) return;
    markedText = nil;
    if (self.windowHandle) ng_invoke_ime_preedit(self.windowHandle, "", -1);
}

- (BOOL)hasMarkedText {
    return markedText != nil && [markedText length] > 0;
}

- (NSRange)markedRange {
    return [self hasMarkedText] ? NSMakeRange(0, [markedText length]) : NSMakeRange(NSNotFound, 0);
}

- (NSRange)selectedRange {
    return NSMakeRange(NSNotFound, 0);
}

- (NSArray<NSAttributedStringKey>*)validAttributesForMarkedText {
    return @[];
}

- (NSAttributedString*)attributedSubstringForProposedRange:(NSRange)range actualRange:(NSRangePointer)actualRange {
    (void)range;
    (void)actualRange;
    return nil;
}

- (NSUInteger)characterIndexForPoint:(NSPoint)point {
    (void)point;
    return NSNotFound;
}

- (NSRect)firstRectForCharacterRange:(NSRange)range actualRange:(NSRangePointer)actualRange {
    (void)range;
    (void)actualRange;
    CGFloat h = [self bounds].size.height;
    NSRect local = NSMakeRect(
        imeArea.origin.x,
        h - imeArea.origin.y - imeArea.size.height,
        imeArea.size.width,
        imeArea.size.height);
    NSRect inWindow = [self convertRect:local toView:nil];
    return [self window] ? [[self window] convertRectToScreen:inWindow] : inWindow;
}

- (void)doCommandBySelector:(SEL)selector {
    (void)selector;
}

- (void)setImeArea:(NSRect)area {
    imeArea = area;
    [[self inputContext] invalidateCharacterCoordinates];
}

- (void)keyUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
    ng_invoke_key_event(self.windowHandle, keycode, 0, ng_macos_modifiers(event));
}

- (void)flagsChanged:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSEventModifierFlags flags = [event modifierFlags];
    unsigned short rawCode = [event keyCode];
    unsigned int keycode = ng_macos_keycode_from_event(rawCode);

    NSEventModifierFlags flag;
    unsigned int bit;
    switch (rawCode) {
        case 56: case 60: flag = NSEventModifierFlagShift;   bit = 1u << 0; break;
        case 59: case 62: flag = NSEventModifierFlagControl; bit = 1u << 1; break;
        case 58: case 61: flag = NSEventModifierFlagOption;  bit = 1u << 2; break;
        case 55: case 54: flag = NSEventModifierFlagCommand; bit = 1u << 3; break;
        default: return;
    }
    int pressed = (flags & flag) ? 1 : 0;
    if (pressed == ((modifiersDown & bit) != 0)) return;
    if (pressed) {
        modifiersDown |= bit;
    } else {
        modifiersDown &= ~bit;
    }

    ng_invoke_key_event(self.windowHandle, keycode, pressed, ng_macos_modifiers(event));
}
@end

@implementation WindowDelegate
- (BOOL)windowShouldClose:(NSWindow*)sender {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 5);
    }
    return YES;
}

- (void)windowDidMiniaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 6);
        ng_invoke_lifecycle_callback(self.windowHandle, 9);
    }
}

- (void)windowDidDeminiaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 7);
        ng_invoke_lifecycle_callback(self.windowHandle, 10);
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
        ng_invoke_lifecycle_callback(self.windowHandle, 11);
    }
}

- (void)windowDidResize:(NSNotification *)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 12);
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
        case 0:
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskMiniaturizable |
                       NSWindowStyleMaskResizable;
            break;
        case 1:
            styleMask = NSWindowStyleMaskBorderless;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 2:
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskUtilityWindow |
                       NSWindowStyleMaskResizable;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 3:
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable |
                       NSWindowStyleMaskUtilityWindow |
                       NSWindowStyleMaskResizable;
            windowLevel = NSFloatingWindowLevel;
            break;
        case 4:
            styleMask = NSWindowStyleMaskTitled |
                       NSWindowStyleMaskClosable;
            isSheet = YES;
            break;
        case 5:
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
    [window setReleasedWhenClosed:NO];
    
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
    
    NSView* mainContentView = [window contentView];
    
    [contentView setFrame:[mainContentView bounds]];
    
    [mainContentView addSubview:contentView];
    
    contentView.translatesAutoresizingMaskIntoConstraints = NO;
    
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

int ng_macos_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
) {
    if (!window || !rgba || width == 0 || height == 0) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc]
        initWithBitmapDataPlanes:NULL
        pixelsWide:(NSInteger)width
        pixelsHigh:(NSInteger)height
        bitsPerSample:8
        samplesPerPixel:4
        hasAlpha:YES
        isPlanar:NO
        colorSpaceName:NSDeviceRGBColorSpace
        bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
        bytesPerRow:(NSInteger)width * 4
        bitsPerPixel:32];
    if (!bitmap) return NG_ERROR_PLATFORM_SPECIFIC;

    memcpy([bitmap bitmapData], rgba, (size_t)width * (size_t)height * 4);
    NSImage* image = [[NSImage alloc]
        initWithSize:NSMakeSize((CGFloat)width, (CGFloat)height)];
    [image addRepresentation:bitmap];
    [NSApp setApplicationIconImage:image];
    return NG_SUCCESS;
}

void ng_macos_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [nsWindow setContentSize:NSMakeSize(width, height)];
}

void ng_macos_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect content = [nsWindow contentRectForFrameRect:[nsWindow frame]];
    *width = (int)content.size.width;
    *height = (int)content.size.height;
}

void ng_macos_window_set_position(NGHandle window, int x, int y) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSScreen* screen = [[NSScreen screens] objectAtIndex:0];
    NSRect screenFrame = [screen frame];
    
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

static id g_shortcut_monitor = nil;

static void ng_macos_ensure_shortcut_monitor(void) {
    if (g_shortcut_monitor) return;
    g_shortcut_monitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown
                                                               handler:^NSEvent*(NSEvent* event) {
        NSWindow* window = [event window];
        if (!window || [window firstResponder] == [window contentView]) return event;
        void* handle = (__bridge void*)window;
        unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
        unsigned int mods = ng_macos_modifiers(event);
        if (!ng_shortcuts_match(handle, keycode, mods)) return event;
        ng_invoke_key_event(handle, keycode, 1, mods);
        return nil;
    }];
}

int ng_macos_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    ng_macos_ensure_shortcut_monitor();
    return ng_shortcuts_set(window, keycodes, modifiers, count);
}

int ng_macos_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* view = [nsWindow contentView];
    if (![view isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;
    [(AureaContentView*)view setImeArea:NSMakeRect(x, y, width, height)];
    return NG_SUCCESS;
}

int ng_macos_window_set_cursor_icon(NGHandle window, int icon) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* view = [nsWindow contentView];
    if (![view isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;

    NSCursor* cursor;
    switch (icon) {
        case 1: cursor = [NSCursor pointingHandCursor]; break;
        case 2: cursor = [NSCursor IBeamCursor]; break;
        case 3: cursor = [NSCursor crosshairCursor]; break;
        case 4: cursor = [NSCursor openHandCursor]; break;
        case 5: cursor = [NSCursor resizeLeftRightCursor]; break;
        case 6: cursor = [NSCursor resizeUpDownCursor]; break;
        case 7: cursor = [NSCursor operationNotAllowedCursor]; break;
        default: cursor = [NSCursor arrowCursor]; break;
    }
    ((AureaContentView*)view).cursor = cursor;
    [nsWindow invalidateCursorRectsForView:view];
    [cursor set];
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

void ng_macos_destroy_element(NGHandle element) {
    if (!element) return;
    NSView* view = (__bridge_transfer NSView*)element;
    if ([view isKindOfClass:[NSView class]] && [view superview]) {
        [view removeFromSuperview];
    }
}

void ng_macos_element_released_to_parent(NGHandle element) {
    if (!element) return;
    NSView* view = (__bridge_transfer NSView*)element;
    (void)view;
}

int ng_macos_detach_element(NGHandle element) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    NSView* view = (__bridge NSView*)element;
    if (![view isKindOfClass:[NSView class]]) return NG_ERROR_INVALID_HANDLE;

    if ([view superview]) {
        CFTypeRef reclaimed = (__bridge_retained CFTypeRef)view;
        (void)reclaimed;
        [view removeFromSuperview];
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

char* ng_macos_get_clipboard_text(void) {
    NSPasteboard* pb = [NSPasteboard generalPasteboard];
    NSString* str = [pb stringForType:NSPasteboardTypeString];
    if (!str) return NULL;
    const char* utf8 = [str UTF8String];
    if (!utf8) return NULL;
    return strdup(utf8);
}

void ng_macos_free_clipboard_text(char* text) {
    free(text);
}

int ng_macos_set_clipboard_text(const char* text) {
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    NSPasteboard* pb = [NSPasteboard generalPasteboard];
    [pb clearContents];
    NSString* str = [NSString stringWithUTF8String:text];
    if (!str) return NG_ERROR_PLATFORM_SPECIFIC;
    BOOL ok = [pb setString:str forType:NSPasteboardTypeString];
    return ok ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}
