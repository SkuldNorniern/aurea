#import "window.h"
#import "content_view.h"
#import "pen.h"
#import "raw.h"
#import "gesture.h"
#import "drop.h"
#import "menu.h"
#import "text.h"
#import "common/errors.h"
#import "common/input.h"
#import "common/rust_callbacks.h"
#import "common/types.h"
#import "common/shortcuts.h"
#import "utils.h"
#import "elements.h"
#import "../macos.h"
#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>
#import <QuartzCore/QuartzCore.h>
#include <string.h>
#import <objc/runtime.h>

static void ng_macos_notify_destroyed(NSView* view);
static void ng_macos_cursor_refresh(NSWindow* window);
static void ng_macos_cursor_confine(NSWindow* window);
#include <limits.h>


@interface WindowDelegate : NSObject <NSWindowDelegate>
@property (nonatomic, assign) void* windowHandle;
@property (nonatomic, assign) ScaleFactorCallback scaleFactorCallback;
@property (nonatomic, assign) BOOL lifecycleCallbackEnabled;
@property (nonatomic, assign) BOOL hideOnClose;
@property (nonatomic, assign) BOOL closeGuarded;
@end

unsigned int ng_macos_modifiers(NSEvent* event) {
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

/* The modifiers of a mouse event, with what moved the pointer: a tablet's events are pens. */
unsigned int ng_macos_mouse_modifiers(NSEvent* event) {
    unsigned int mods = ng_macos_modifiers(event);
    mods |= ng_macos_pen_bits(event);
    return mods;
}

unsigned int ng_macos_keycode_from_event(unsigned short keycode) {
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
        case 36:
        case 76: return NG_KEY_ENTER;
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


@implementation AureaContentView

- (CALayer*)makeBackingLayer {
    if (self.usesMetalLayer) return [CAMetalLayer layer];
    return [super makeBackingLayer];
}

- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)info {
    return ng_macos_drop_hover(self, info);
}

- (NSDragOperation)draggingUpdated:(id<NSDraggingInfo>)info {
    return ng_macos_drop_hover(self, info);
}

- (void)draggingExited:(id<NSDraggingInfo>)info {
    (void)info;
    ng_macos_drop_leave(self);
}

- (BOOL)performDragOperation:(id<NSDraggingInfo>)info {
    return ng_macos_drop_perform(self, info);
}

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
    ng_macos_cursor_refresh([self window]);
    if (exitHeld) {
        exitHeld = NO;
        return;
    }
    if (self.windowHandle) {
        ng_invoke_cursor_entered(self.windowHandle, 1);
    }
}

- (void)mouseExited:(NSEvent*)event {
    ng_macos_cursor_confine([self window]);
    ng_macos_cursor_refresh([self window]);
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

- (void)tabletProximity:(NSEvent*)event {
    if (!ng_macos_pen_proximity(event)) [super tabletProximity:event];
}

- (void)tabletPoint:(NSEvent*)event {
    if (!ng_macos_pen_point(self, event)) [super tabletPoint:event];
}

- (void)mouseMoved:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint location = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    unsigned int buttons = (unsigned int)([NSEvent pressedMouseButtons] & 0xFF);
    ng_invoke_mouse_move(self.windowHandle, location.x, h - location.y, buttons, ng_macos_mouse_modifiers(event));
    ng_macos_pen_report(self, event, ((buttons & 1) ? AUREA_PEN_MOVE : AUREA_PEN_HOVER));

    if (self.grabMode == 1) ng_macos_cursor_confine([self window]);
    if (self.grabMode == 2) ng_macos_raw_motion(self, event);
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
    ng_macos_text_commit(self, event);
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 0, 1, ng_macos_mouse_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    ng_macos_pen_report(self, event, AUREA_PEN_DOWN);
    [self buttonWentDown];
}

- (void)mouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 0, 0, ng_macos_mouse_modifiers(event), p.x, h - p.y, 1);
    ng_macos_pen_report(self, event, AUREA_PEN_UP);
    [self buttonWentUp];
}

- (void)rightMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_macos_text_commit(self, event);
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 1, 1, ng_macos_mouse_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    [self buttonWentDown];
}

- (void)rightMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, 1, 0, ng_macos_mouse_modifiers(event), p.x, h - p.y, 1);
    [self buttonWentUp];
}

- (void)otherMouseDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    ng_macos_text_commit(self, event);
    int button = (int)[event buttonNumber];
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, button, 1, ng_macos_mouse_modifiers(event), p.x, h - p.y, (int)[event clickCount]);
    [self buttonWentDown];
}

- (void)otherMouseUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    int button = (int)[event buttonNumber];
    NSPoint p = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat h = [self bounds].size.height;
    ng_invoke_mouse_button(self.windowHandle, button, 0, ng_macos_mouse_modifiers(event), p.x, h - p.y, 1);
    [self buttonWentUp];
}

/* Where a trackpad scroll falls in its gesture; -1 for the touch down before any move. */
static int ng_macos_scroll_phase(NSEvent* event) {
    NSEventPhase momentum = event.momentumPhase;
    if (momentum != NSEventPhaseNone) {
        if (momentum & (NSEventPhaseEnded | NSEventPhaseCancelled)) return NG_SCROLL_MOMENTUM_ENDED;
        return NG_SCROLL_MOMENTUM;
    }
    switch (event.phase) {
        case NSEventPhaseNone: return NG_SCROLL_DISCRETE;
        case NSEventPhaseMayBegin: return -1;
        case NSEventPhaseBegan: return NG_SCROLL_BEGAN;
        case NSEventPhaseChanged: return NG_SCROLL_MOVED;
        default: return NG_SCROLL_ENDED;
    }
}

/* AppKit deltas follow the content: right is negative x there, up positive y. A wheel with
   detents gives lines and goes as clicks; a trackpad gives points, which are logical pixels. */
- (void)scrollWheel:(NSEvent*)event {
    if (!self.windowHandle) return;
    double dx = -(double)[event scrollingDeltaX];
    double dy = (double)[event scrollingDeltaY];
    unsigned int mods = ng_macos_modifiers(event);
    int inverted = event.isDirectionInvertedFromDevice ? 1 : 0;
    if (![event hasPreciseScrollingDeltas]) {
        ng_invoke_scroll(self.windowHandle, 0.0, 0.0, 1, dx, dy, NG_SCROLL_WHEEL,
                         NG_SCROLL_DISCRETE, inverted, mods);
        return;
    }
    int phase = ng_macos_scroll_phase(event);
    if (phase < 0) return;
    ng_invoke_scroll(self.windowHandle, dx, dy, 0, 0.0, 0.0, NG_SCROLL_TRACKPAD, (unsigned int)phase,
                     inverted, mods);
}

- (void)magnifyWithEvent:(NSEvent*)event {
    if (!ng_macos_gesture_magnify(self, event)) [super magnifyWithEvent:event];
}

- (void)rotateWithEvent:(NSEvent*)event {
    if (!ng_macos_gesture_rotate(self, event)) [super rotateWithEvent:event];
}

- (void)sendKeyDown:(NSEvent*)event {
    if (!keysDown) keysDown = [NSMutableIndexSet indexSet];
    [keysDown addIndex:[event keyCode]];
    unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
    ng_invoke_key_event(self.windowHandle, keycode, 1, ng_macos_modifiers(event));
}

- (void)keyDown:(NSEvent*)event {
    if (!self.windowHandle) return;
    BOOL composing = ng_macos_text_composing(self);
    if (!composing) {
        if (ng_macos_menu_key(event)) return;
        [self sendKeyDown:event];
    }
    /* Korean commits on Enter, arrows, Tab and sends the key on as a command after the text.
       the key goes out then, after the commit, so the newline or caret move is not lost */
    if (ng_macos_text_key(self, event) && composing) [self sendKeyDown:event];
}

- (void)keyUp:(NSEvent*)event {
    if (!self.windowHandle) return;
    /* no release for a press the input method or the menu took */
    if (![keysDown containsIndex:[event keyCode]]) return;
    [keysDown removeIndex:[event keyCode]];
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

static void ng_macos_cursor_grab_focus(NSWindow* window, BOOL focused);
static void ng_macos_forget_held_shortcuts(void* window);

@implementation WindowDelegate
- (BOOL)windowShouldClose:(NSWindow*)sender {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 5);
    }
    if (self.closeGuarded) {
        return NO;
    }
    if (self.hideOnClose) {
        [sender orderOut:nil];
        return NO;
    }
    return YES;
}

- (void)windowDidMiniaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 6);
        ng_invoke_lifecycle_callback(self.windowHandle, 9);
    }
}

- (void)windowDidChangeOcclusionState:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle,
                                     13 + ng_macos_window_occlusion(self.windowHandle));
    }
}

- (void)windowDidDeminiaturize:(NSNotification*)notification {
    if (self.lifecycleCallbackEnabled && self.windowHandle) {
        ng_invoke_lifecycle_callback(self.windowHandle, 7);
        ng_invoke_lifecycle_callback(self.windowHandle, 10);
    }
}

- (void)windowDidChangeScreen:(NSNotification*)notification {
    if ([[notification object] isKeyWindow]) ng_macos_display_link_follow([[notification object] screen]);
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
    ng_macos_cursor_refresh([notification object]);
    ng_macos_display_link_follow([[notification object] screen]);
    ng_macos_cursor_grab_focus([notification object], YES);
    if (self.windowHandle) {
        ng_invoke_focus_changed(self.windowHandle, 1);
    }
}

- (void)windowDidResignKey:(NSNotification*)notification {
    ng_macos_cursor_refresh([notification object]);
    ng_macos_cursor_grab_focus([notification object], NO);
    NSView* content = [[notification object] contentView];
    if ([content isKindOfClass:[AureaContentView class]]) {
        ng_macos_text_drop((AureaContentView*)content);
    }
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

/* the Aurea view of a window, nil for any other content */
static AureaContentView* ng_macos_view(NSWindow* window) {
    NSView* content = [window contentView];
    return [content isKindOfClass:[AureaContentView class]] ? (AureaContentView*)content : nil;
}

/* the delegate an Aurea window's view keeps, nil for any other window */
static WindowDelegate* ng_macos_delegate(NSWindow* window) {
    return (WindowDelegate*)ng_macos_view(window).windowDelegate;
}

static BOOL s_cursor_hidden = NO;

static void ng_macos_cursor_set_hidden(BOOL hidden) {
    if (hidden == s_cursor_hidden) return;
    s_cursor_hidden = hidden;
    if (hidden) {
        CGDisplayHideCursor(kCGDirectMainDisplay);
    } else {
        CGDisplayShowCursor(kCGDirectMainDisplay);
    }
}

static BOOL ng_macos_pointer_inside(NSWindow* window) {
    NSView* content = [window contentView];
    NSPoint p = [content convertPoint:[window mouseLocationOutsideOfEventStream] fromView:nil];
    return NSPointInRect(p, [content bounds]);
}

static void ng_macos_cursor_refresh(NSWindow* window) {
    NSView* content = [window contentView];
    if (![content isKindOfClass:[AureaContentView class]]) return;
    BOOL wants = ((AureaContentView*)content).hidesCursor;
    if (!wants && window != [NSApp keyWindow]) return;
    ng_macos_cursor_set_hidden(wants && [window isKeyWindow] && ng_macos_pointer_inside(window));
}

static void ng_macos_warp_cursor(NSPoint cocoa) {
    CGFloat primaryHeight = NSMaxY([[[NSScreen screens] firstObject] frame]);
    CGWarpMouseCursorPosition(CGPointMake(cocoa.x, primaryHeight - cocoa.y));
}

static BOOL ng_macos_cursor_locked(NSWindow* window) {
    return ng_macos_view(window).grabMode == 2;
}

static void ng_macos_cursor_grab_focus(NSWindow* window, BOOL focused) {
    if (!window || !ng_macos_cursor_locked(window)) return;
    CGAssociateMouseAndMouseCursorPosition(focused ? false : true);
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
    
    [window setTitle:ng_macos_to_nsstring(title)];

    /* A window asked bigger than the screen's visible frame would hide its own edges. */
    NSScreen* screen = window.screen ? window.screen : [NSScreen mainScreen];
    if (screen) {
        NSRect visible = screen.visibleFrame;
        NSRect outer = window.frame;
        if (outer.size.width > visible.size.width || outer.size.height > visible.size.height) {
            outer.size.width = MIN(outer.size.width, visible.size.width);
            outer.size.height = MIN(outer.size.height, visible.size.height);
            [window setFrame:outer display:NO];
        }
    }

    if (!isSheet) {
        [window center];
    }

    [window setAcceptsMouseMovedEvents:YES];
    AureaContentView* contentView = [[AureaContentView alloc] initWithFrame:frame];
    contentView.windowHandle = (__bridge void*)window;
    contentView.windowDelegate = delegate;
    [contentView setWantsLayer:YES];
    ng_macos_drop_register(contentView);
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
    [window setDelegate:nil];
    WindowDelegate* delegate = ng_macos_delegate(window);
    if (delegate.hideOnClose) ng_macos_hide_on_close_count--;
    ng_macos_view(window).windowDelegate = nil;
    if (ng_macos_cursor_locked(window)) {
        CGAssociateMouseAndMouseCursorPosition(true);
    }
    if ([[window contentView] isKindOfClass:[AureaContentView class]]
        && ((AureaContentView*)[window contentView]).hidesCursor) {
        ng_macos_cursor_set_hidden(NO);
    }
    ng_shortcuts_clear(handle);
    ng_macos_forget_held_shortcuts(handle);
    ng_macos_notify_destroyed([window contentView]);
    [window close];
}

void ng_macos_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    WindowDelegate* delegate = ng_macos_delegate(nsWindow);
    if (delegate) {
        delegate.scaleFactorCallback = callback;
    }
}

void ng_macos_window_set_lifecycle_callback(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    WindowDelegate* delegate = ng_macos_delegate(nsWindow);
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
    NSBitmapImageRep* bitmap = ng_macos_rgba_bitmap(rgba, width, height);
    if (!bitmap) return NG_ERROR_PLATFORM_SPECIFIC;
    NSImage* image = [[NSImage alloc]
        initWithSize:NSMakeSize((CGFloat)width, (CGFloat)height)];
    [image addRepresentation:bitmap];
    [NSApp setApplicationIconImage:image];
    return NG_SUCCESS;
}

void ng_macos_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    /* setContentSize: skips the minimum a user resize keeps. */
    NSSize min = nsWindow.contentMinSize;
    [nsWindow setContentSize:NSMakeSize(MAX(width, min.width), MAX(height, min.height))];
}

int ng_macos_window_set_min_size(NGHandle window, int width, int height) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [nsWindow setContentMinSize:NSMakeSize(width, height)];
    NSSize size = [nsWindow contentRectForFrameRect:[nsWindow frame]].size;
    if (size.width < width || size.height < height) {
        [nsWindow setContentSize:NSMakeSize(MAX(size.width, width), MAX(size.height, height))];
    }
    return NG_SUCCESS;
}

void ng_macos_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect content = [nsWindow contentRectForFrameRect:[nsWindow frame]];
    *width = (int)lround(content.size.width);
    *height = (int)lround(content.size.height);
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

int ng_macos_window_refresh_rate(NGHandle window) {
    if (!window) return 0;
    NSScreen* screen = [(__bridge NSWindow*)window screen] ?: [NSScreen mainScreen];
    if (@available(macOS 12.0, *)) return (int)[screen maximumFramesPerSecond] * 1000;
    return 0;
}

int ng_macos_window_request_attention(NGHandle window, int critical) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    /* the dock bounces once, or until the app is active when critical; nothing when it is */
    [NSApp requestUserAttention:critical ? NSCriticalRequest : NSInformationalRequest];
    return NG_SUCCESS;
}

int ng_macos_window_get_placement(NGHandle window, int* rect) {
    if (!window || !rect) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSRect frame = [nsWindow frame];
    CGFloat top = [[[NSScreen screens] objectAtIndex:0] frame].size.height;
    rect[0] = (int)frame.origin.x;
    rect[1] = (int)(top - (frame.origin.y + frame.size.height));
    rect[2] = (int)frame.size.width;
    rect[3] = (int)frame.size.height;
    return ([nsWindow isZoomed] ? 1 : 0) | ([nsWindow isMiniaturized] ? 2 : 0);
}

int ng_macos_window_set_placement(NGHandle window, const int* rect, int maximized) {
    if (!window || !rect) return NG_ERROR_INVALID_HANDLE;
    if (rect[2] <= 0 || rect[3] <= 0) return NG_ERROR_INVALID_PARAMETER;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    CGFloat top = [[[NSScreen screens] objectAtIndex:0] frame].size.height;
    NSRect frame = NSMakeRect(rect[0], top - rect[1] - rect[3], rect[2], rect[3]);
    NSScreen* on = nil;
    for (NSScreen* screen in [NSScreen screens]) {
        if (NSIntersectsRect([screen visibleFrame], frame)) {
            on = screen;
            break;
        }
    }
    if (!on) {
        /* off every screen now, as after unplugging one: centre it on the main screen */
        NSRect visible = [[NSScreen mainScreen] visibleFrame];
        frame.origin.x = NSMidX(visible) - frame.size.width / 2;
        frame.origin.y = NSMidY(visible) - frame.size.height / 2;
        on = [NSScreen mainScreen];
    }
    if ([nsWindow isZoomed]) [nsWindow zoom:nil];
    [nsWindow setFrame:[nsWindow constrainFrameRect:frame toScreen:on] display:YES];
    if (maximized && ![nsWindow isZoomed]) [nsWindow zoom:nil];
    return NG_SUCCESS;
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
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* content = [nsWindow contentView];
    if (![content isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;
    ((AureaContentView*)content).hidesCursor = visible ? NO : YES;
    if (visible && s_cursor_hidden) {
        ng_macos_cursor_set_hidden(NO);
    }
    ng_macos_cursor_refresh(nsWindow);
    return NG_SUCCESS;
}

static void ng_macos_cursor_confine(NSWindow* window) {
    if (!window || ![window isKeyWindow]) return;
    if (ng_macos_view(window).grabMode != 1) return;
    NSRect area = NSInsetRect([window convertRectToScreen:[[window contentView] frame]], 1, 1);
    if (NSIsEmptyRect(area)) return;
    NSPoint p = [NSEvent mouseLocation];
    if (NSPointInRect(p, area)) return;
    NSPoint clamped = NSMakePoint(MIN(MAX(p.x, NSMinX(area)), NSMaxX(area) - 1),
                                  MIN(MAX(p.y, NSMinY(area) + 1), NSMaxY(area)));
    ng_macos_warp_cursor(clamped);
    CGAssociateMouseAndMouseCursorPosition(true);
}

static id g_shortcut_monitor = nil;

#define NG_SHORTCUTS_HELD 8
static struct { void* window; unsigned int keycode; } g_shortcuts_held[NG_SHORTCUTS_HELD];

static BOOL ng_macos_take_held_shortcut(void* window, unsigned int keycode) {
    for (int i = 0; i < NG_SHORTCUTS_HELD; i++) {
        if (g_shortcuts_held[i].window == window && g_shortcuts_held[i].keycode == keycode) {
            g_shortcuts_held[i].window = NULL;
            return YES;
        }
    }
    return NO;
}

static void ng_macos_hold_shortcut(void* window, unsigned int keycode) {
    for (int i = 0; i < NG_SHORTCUTS_HELD; i++) {
        if (!g_shortcuts_held[i].window) {
            g_shortcuts_held[i].window = window;
            g_shortcuts_held[i].keycode = keycode;
            return;
        }
    }
}

static void ng_macos_forget_held_shortcuts(void* window) {
    for (int i = 0; i < NG_SHORTCUTS_HELD; i++) {
        if (g_shortcuts_held[i].window == window) g_shortcuts_held[i].window = NULL;
    }
}

static void ng_macos_ensure_shortcut_monitor(void) {
    if (g_shortcut_monitor) return;
    NSEventMask mask = NSEventMaskKeyDown | NSEventMaskKeyUp | NSEventMaskFlagsChanged;
    g_shortcut_monitor = [NSEvent addLocalMonitorForEventsMatchingMask:mask
                                                               handler:^NSEvent*(NSEvent* event) {
        NSWindow* window = [event window];
        NSView* content = [window contentView];
        if (!window || [window firstResponder] == content) return event;
        void* handle = (__bridge void*)window;
        if ([event type] == NSEventTypeFlagsChanged) {
            if ([content isKindOfClass:[AureaContentView class]]) {
                [(AureaContentView*)content flagsChanged:event];
            }
            return event;
        }
        unsigned int keycode = ng_macos_keycode_from_event([event keyCode]);
        unsigned int mods = ng_macos_modifiers(event);
        if ([event type] == NSEventTypeKeyUp) {
            if (!ng_macos_take_held_shortcut(handle, keycode)) return event;
            ng_invoke_key_event(handle, keycode, 0, mods);
            return nil;
        }
        if (!ng_shortcuts_match(handle, keycode, mods)) return event;
        if (![event isARepeat]) {
            ng_macos_hold_shortcut(handle, keycode);
        }
        ng_invoke_key_event(handle, keycode, 1, mods);
        return nil;
    }];
}

int ng_macos_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    ng_macos_ensure_shortcut_monitor();
    return ng_shortcuts_set(window, keycodes, modifiers, count);
}

static NSCursor* diagonalResizeCursor(BOOL reverse) {
    static NSCursor* cursors[2] = { nil, nil };
    if (cursors[reverse]) return cursors[reverse];
    NSImage* image = [[NSImage alloc] initWithSize:NSMakeSize(16, 16)];
    [image lockFocus];
    NSBezierPath* path = [NSBezierPath bezierPath];
    NSPoint a = NSMakePoint(reverse ? 13 : 3, 13);
    NSPoint b = NSMakePoint(reverse ? 3 : 13, 3);
    [path moveToPoint:a]; [path lineToPoint:b];
    [path moveToPoint:NSMakePoint(a.x, 8)]; [path lineToPoint:a]; [path lineToPoint:NSMakePoint(8, 13)];
    [path moveToPoint:NSMakePoint(b.x, 8)]; [path lineToPoint:b]; [path lineToPoint:NSMakePoint(8, 3)];
    path.lineCapStyle = NSLineCapStyleRound;
    path.lineJoinStyle = NSLineJoinStyleRound;
    [[NSColor blackColor] setStroke]; path.lineWidth = 3; [path stroke];
    [[NSColor whiteColor] setStroke]; path.lineWidth = 1; [path stroke];
    [image unlockFocus];
    cursors[reverse] = [[NSCursor alloc] initWithImage:image hotSpot:NSMakePoint(8, 8)];
    return cursors[reverse];
}

int ng_macos_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y) {
    @autoreleasepool {
        if (!window || !rgba || !width || !height || hot_x >= width || hot_y >= height || width > INT_MAX / 4 || height > INT_MAX) return NG_ERROR_INVALID_PARAMETER;
        NSWindow* nsWindow = (__bridge NSWindow*)window;
        NSView* view = nsWindow.contentView;
        if (![view isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;
        NSBitmapImageRep* bitmap = ng_macos_rgba_bitmap(rgba, width, height);
        if (!bitmap) return NG_ERROR_PLATFORM_SPECIFIC;
        CGFloat scale = MAX(1.0, nsWindow.backingScaleFactor);
        NSImage* image = [[NSImage alloc] initWithSize:NSMakeSize(width / scale, height / scale)];
        [image addRepresentation:bitmap];
        NSCursor* cursor = [[NSCursor alloc] initWithImage:image hotSpot:NSMakePoint(hot_x / scale, hot_y / scale)];
        ((AureaContentView*)view).cursor = cursor;
        [nsWindow invalidateCursorRectsForView:view];
        [cursor set];
        return NG_SUCCESS;
    }
}

int ng_macos_window_set_cursor_icon(NGHandle window, int icon) {
    @autoreleasepool {
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
            case 9: cursor = diagonalResizeCursor(NO); break;
            case 10: cursor = diagonalResizeCursor(YES); break;
            default: cursor = [NSCursor arrowCursor]; break;
        }
        ((AureaContentView*)view).cursor = cursor;
        [nsWindow invalidateCursorRectsForView:view];
        [cursor set];
        return NG_SUCCESS;
    }
}

int ng_macos_window_set_cursor_grab(NGHandle window, int mode) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    AureaContentView* view = ng_macos_view(nsWindow);
    if (!view) return NG_ERROR_PLATFORM_SPECIFIC;
    view.grabMode = mode;

    if (mode == 2) {
        view.rawSuppress = YES;
        CGAssociateMouseAndMouseCursorPosition(false);
        NSRect frame = [nsWindow frame];
        ng_macos_warp_cursor(NSMakePoint(NSMidX(frame), NSMidY(frame)));
        CGAssociateMouseAndMouseCursorPosition(false);
    } else {
        CGAssociateMouseAndMouseCursorPosition(true);
        view.rawSuppress = NO;
    }

    return NG_SUCCESS;
}

void ng_macos_destroy_element(NGHandle element) {
    if (!element) return;
    NSView* view = (__bridge_transfer NSView*)element;
    if ([view isKindOfClass:[NSView class]]) ng_macos_notify_destroyed(view);
    if ([view isKindOfClass:[NSView class]] && [view superview]) {
        [view removeFromSuperview];
    }
}

void ng_macos_element_destroyed(NGHandle element) {
    if (element) ng_macos_notify_destroyed((__bridge NSView*)element);
}

void ng_macos_element_released_to_parent(NGHandle element) {
    if (!element) return;
    NSView* view = (__bridge_transfer NSView*)element;
    (void)view;
}

void ng_macos_element_retain(NGHandle element) {
    if (element) CFRetain(element);
}

void ng_macos_element_release(NGHandle element) {
    if (element) CFRelease(element);
}

static char kChangeIdKey;
static char kDestroyIdKey;
static char kEditableKey;
static char kTextColorKey;

static void ng_macos_enable_view(NSView* view, BOOL enabled) {
    if ([view isKindOfClass:[NSControl class]]) {
        [(NSControl*)view setEnabled:enabled];
    } else if ([view isKindOfClass:[NSTextView class]]) {
        NSTextView* text = (NSTextView*)view;
        NSNumber* was = objc_getAssociatedObject(text, &kEditableKey);
        if (!enabled && !was) {
            objc_setAssociatedObject(text, &kEditableKey, @([text isEditable]), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            objc_setAssociatedObject(text, &kTextColorKey, [text textColor], OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            [text setEditable:NO];
            [text setSelectable:NO];
            [text setTextColor:[NSColor disabledControlTextColor]];
        } else if (enabled && was) {
            NSColor* color = objc_getAssociatedObject(text, &kTextColorKey);
            objc_setAssociatedObject(text, &kEditableKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            objc_setAssociatedObject(text, &kTextColorKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            [text setEditable:[was boolValue]];
            [text setSelectable:YES];
            [text setTextColor:color ?: [NSColor textColor]];
        }
    }
    for (NSView* sub in [view subviews]) ng_macos_enable_view(sub, enabled);
}

int ng_macos_element_set_enabled(NGHandle element, int enabled) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    ng_macos_enable_view((__bridge NSView*)element, enabled ? YES : NO);
    return NG_SUCCESS;
}

int ng_macos_element_watch_destroy(NGHandle element, unsigned int id) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    objc_setAssociatedObject((__bridge NSView*)element, &kDestroyIdKey, @(id), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    return NG_SUCCESS;
}

static void ng_macos_notify_destroyed(NSView* view) {
    if (!view) return;
    for (NSView* sub in [[view subviews] copy]) ng_macos_notify_destroyed(sub);
    NSNumber* watch = objc_getAssociatedObject(view, &kDestroyIdKey);
    if (watch) {
        objc_setAssociatedObject(view, &kDestroyIdKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        ng_invoke_custom_callback(watch.unsignedIntValue);
    }
}
static char kMinWidthKey;
static char kMinHeightKey;

@interface NGChangeTarget : NSObject
- (void)changed:(id)sender;
@end

@implementation NGChangeTarget
- (void)changed:(id)sender {
    NSNumber* changeId = objc_getAssociatedObject(sender, &kChangeIdKey);
    if (changeId) ng_invoke_custom_callback(changeId.unsignedIntValue);
}
@end

int ng_macos_element_set_change_id(NGHandle element, unsigned int id) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (ng_macos_text_watch_change(element, id)) return NG_SUCCESS;
    if (ng_macos_spin_box_watch_change(element, id)) return NG_SUCCESS;
    NSView* view = (__bridge NSView*)element;
    if (![view isKindOfClass:[NSControl class]]) return NG_ERROR_UNSUPPORTED;
    static NGChangeTarget* target = nil;
    if (!target) target = [[NGChangeTarget alloc] init];
    NSControl* control = (NSControl*)view;
    objc_setAssociatedObject(control, &kChangeIdKey, @(id), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    [control setTarget:target];
    [control setAction:@selector(changed:)];
    if ([control isKindOfClass:[NSSlider class]]) [control setContinuous:YES];
    return NG_SUCCESS;
}

int ng_macos_element_set_visible(NGHandle element, int visible) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    [(__bridge NSView*)element setHidden:visible ? NO : YES];
    return NG_SUCCESS;
}

static void ng_macos_min_constraint(NSView* view, void* key, NSLayoutDimension* dimension, int value) {
    NSLayoutConstraint* old = objc_getAssociatedObject(view, key);
    if (old) old.active = NO;
    NSLayoutConstraint* min = nil;
    if (value > 0) {
        min = [dimension constraintGreaterThanOrEqualToConstant:value];
        min.priority = NSLayoutPriorityRequired - 1;
        min.active = YES;
    }
    objc_setAssociatedObject(view, key, min, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

int ng_macos_element_set_min_size(NGHandle element, int width, int height) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    NSView* view = (__bridge NSView*)element;
    ng_macos_min_constraint(view, &kMinWidthKey, view.widthAnchor, width);
    ng_macos_min_constraint(view, &kMinHeightKey, view.heightAnchor, height);
    return NG_SUCCESS;
}

int ng_macos_element_set_tooltip(NGHandle element, const char* text) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    ((__bridge NSView*)element).toolTip = *text ? [NSString stringWithUTF8String:text] : nil;
    return NG_SUCCESS;
}

int ng_macos_element_set_accessible_name(NGHandle element, const char* name) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (!name) return NG_ERROR_INVALID_PARAMETER;
    NSView* view = (__bridge NSView*)element;
    if ([view isKindOfClass:[NSScrollView class]] &&
        ([((NSScrollView*)view).documentView isKindOfClass:[NSTextView class]] ||
         [((NSScrollView*)view).documentView isKindOfClass:[NSTableView class]])) {
        view = ((NSScrollView*)view).documentView;
    }
    view.accessibilityLabel = *name ? [NSString stringWithUTF8String:name] : nil;
    return NG_SUCCESS;
}

int ng_macos_box_set_spacing(NGHandle box, int spacing) {
    if (!box) return NG_ERROR_INVALID_HANDLE;
    NSView* view = (__bridge NSView*)box;
    if (![view isKindOfClass:[NSStackView class]]) return NG_ERROR_INVALID_HANDLE;
    if (spacing < 0) return NG_ERROR_INVALID_PARAMETER;
    [(NSStackView*)view setSpacing:spacing];
    return NG_SUCCESS;
}

int ng_macos_box_set_padding(NGHandle box, int padding) {
    if (!box) return NG_ERROR_INVALID_HANDLE;
    NSView* view = (__bridge NSView*)box;
    if (![view isKindOfClass:[NSStackView class]]) return NG_ERROR_INVALID_HANDLE;
    if (padding < 0) return NG_ERROR_INVALID_PARAMETER;
    [(NSStackView*)view setEdgeInsets:NSEdgeInsetsMake(padding, padding, padding, padding)];
    return NG_SUCCESS;
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

void ng_macos_window_set_close_guard(NGHandle window, int guarded) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    WindowDelegate* delegate = ng_macos_delegate(nsWindow);
    if (!delegate) return;
    delegate.closeGuarded = guarded != 0;
}

int ng_macos_hide_on_close_count = 0;

int ng_macos_request_close_all(void) {
    int asked = 0;
    for (NSWindow* window in [[NSApp windows] copy]) {
        WindowDelegate* delegate = ng_macos_delegate(window);
        if (!delegate || !delegate.lifecycleCallbackEnabled || !delegate.windowHandle) continue;
        ng_invoke_lifecycle_callback(delegate.windowHandle, 5);
        asked++;
    }
    return asked;
}

int ng_macos_window_set_hide_on_close(NGHandle window, int hide) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    WindowDelegate* delegate = ng_macos_delegate(nsWindow);
    if (!delegate) return NG_ERROR_INVALID_HANDLE;
    BOOL want = hide != 0;
    if (want != delegate.hideOnClose) ng_macos_hide_on_close_count += want ? 1 : -1;
    delegate.hideOnClose = want;
    return NG_SUCCESS;
}

void ng_macos_window_focus(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    if (nsWindow.isMiniaturized) [nsWindow deminiaturize:nil];
    ng_macos_activate();
    [nsWindow makeKeyAndOrderFront:nil];
}

void ng_macos_window_show(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    ng_macos_activate();
    [nsWindow makeKeyAndOrderFront:nil];
    NSView* contentView = [nsWindow contentView];
    id responder = [nsWindow firstResponder];
    if (contentView && (!responder || responder == nsWindow)) {
        [nsWindow makeFirstResponder:contentView];
    }
}

void ng_macos_window_hide(NGHandle window) {
    if (!window) return;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    [nsWindow orderOut:nil];
}

/* AppKit says whether any of the window shows, not how much. */
int ng_macos_window_occlusion(NGHandle window) {
    if (!window) return 2;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    return ([nsWindow occlusionState] & NSWindowOcclusionStateVisible) ? 0 : 2;
}

int ng_macos_window_is_visible(NGHandle window) {
    if (!window) return 0;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    return [nsWindow isVisible] ? 1 : 0;
}

int ng_macos_window_use_metal_layer(NGHandle window) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* view = [nsWindow contentView];
    if (![view isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;
    if ([[view layer] isKindOfClass:[CAMetalLayer class]]) return NG_SUCCESS;
    ((AureaContentView*)view).usesMetalLayer = YES;
    if ([view wantsLayer]) [view setWantsLayer:NO];
    [view setWantsLayer:YES];
    return [[view layer] isKindOfClass:[CAMetalLayer class]] ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

NGHandle ng_macos_window_get_content_view(NGHandle window) {
    if (!window) return NULL;
    NSWindow* nsWindow = (__bridge NSWindow*)window;
    NSView* contentView = [nsWindow contentView];
    return (__bridge void*)contentView;
} 


void ng_apple_device_info(unsigned long long* total, unsigned long long* available, int* thermal,
                          int* low) {
    NSProcessInfo* process = NSProcessInfo.processInfo;
    *total = process.physicalMemory;
    *available = 0;
    *thermal = (int)process.thermalState;
    *low = 0;
    if (@available(macOS 12.0, *)) *low = process.lowPowerModeEnabled ? 1 : 0;
}
