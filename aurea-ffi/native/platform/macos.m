#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import "macos/elements.h"


#import <Cocoa/Cocoa.h>
#import <CoreFoundation/CoreFoundation.h>
#import <CoreVideo/CoreVideo.h>
#import <QuartzCore/QuartzCore.h>
static BOOL app_initialized = FALSE;

@interface AppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}
@end

static AppDelegate* app_delegate = nil;

// ── Modern vsync pump: CADisplayLink (macOS 14+) ────────────────────────────
//
// Fires already on the main run loop (it's scheduled there in ng_macos_init),
// so no dispatch_async hop is needed before touching AppKit/Rust state,
// unlike the legacy CVDisplayLink path below.
API_AVAILABLE(macos(14.0))
@interface NgDisplayLinkProxy : NSObject
@end

API_AVAILABLE(macos(14.0))
@implementation NgDisplayLinkProxy
- (void)onDisplayLink:(CADisplayLink*)link {
    (void)link;
    ng_process_frames();
}
@end

static CADisplayLink* s_display_link API_AVAILABLE(macos(14.0)) = nil;
static NgDisplayLinkProxy* s_display_link_proxy API_AVAILABLE(macos(14.0)) = nil;

// ── Legacy vsync pump: CVDisplayLink (macOS < 14) ───────────────────────────
//
// CVDisplayLink was deprecated in macOS 15 in favor of CADisplayLink via
// NSScreen/NSView/NSWindow.displayLink(target:selector:) (above), but that
// replacement only exists on macOS 14+. This fallback is the only option on
// older systems, so it's kept — gated by `s_using_legacy`, set only when
// @available(macOS 14.0, *) is false — and its deprecation warnings are
// suppressed for exactly this still-necessary branch.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"

static CVDisplayLinkRef s_cv_display_link = NULL;
static volatile BOOL s_cv_link_running = NO;
static BOOL s_using_legacy = NO;

// CVDisplayLink fires on its own thread; hop to the main queue before
// touching any AppKit/Rust state.
static CVReturn legacy_display_link_callback(
    CVDisplayLinkRef link,
    const CVTimeStamp* now,
    const CVTimeStamp* output,
    CVOptionFlags flagsIn,
    CVOptionFlags* flagsOut,
    void* ctx)
{
    (void)link;
    (void)now;
    (void)output;
    (void)flagsIn;
    (void)flagsOut;
    (void)ctx;
    dispatch_async(dispatch_get_main_queue(), ^{
        ng_process_frames();
    });
    return kCVReturnSuccess;
}

static void legacy_display_link_init(void) {
    s_using_legacy = YES;
    CVDisplayLinkCreateWithActiveCGDisplays(&s_cv_display_link);
    if (s_cv_display_link) {
        CVDisplayLinkSetOutputCallback(s_cv_display_link, legacy_display_link_callback, NULL);
    }
}

static void legacy_display_link_cleanup(void) {
    if (s_cv_display_link) {
        if (s_cv_link_running) {
            CVDisplayLinkStop(s_cv_display_link);
            s_cv_link_running = NO;
        }
        CVDisplayLinkRelease(s_cv_display_link);
        s_cv_display_link = NULL;
    }
}

static void legacy_display_link_request_frame(void) {
    if (s_cv_display_link && !s_cv_link_running) {
        CVDisplayLinkStart(s_cv_display_link);
        s_cv_link_running = YES;
    }
}

static void legacy_display_link_frame_idle(void) {
    if (s_cv_display_link && s_cv_link_running) {
        CVDisplayLinkStop(s_cv_display_link);
        s_cv_link_running = NO;
    }
}
#pragma clang diagnostic pop

int ng_macos_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];

        app_delegate = [[AppDelegate alloc] init];
        [NSApp setDelegate:app_delegate];

        [NSApp finishLaunching];
        // Activate once at startup so the first window appears in front.
        [NSApp activateIgnoringOtherApps:YES];

        if (@available(macOS 14.0, *)) {
            s_display_link_proxy = [[NgDisplayLinkProxy alloc] init];
            NSScreen* screen = [NSScreen mainScreen];
            if (screen) {
                s_display_link = [screen displayLinkWithTarget:s_display_link_proxy
                                                        selector:@selector(onDisplayLink:)];
                // Starts paused; ng_macos_request_frame unpauses on the first
                // scheduled frame, ng_macos_frame_idle pauses again once the
                // scheduler has nothing pending.
                s_display_link.paused = YES;
                [s_display_link addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
            }
        } else {
            legacy_display_link_init();
        }

        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_macos_cleanup(void) {
    if (app_initialized) {
        if (s_using_legacy) {
            legacy_display_link_cleanup();
        } else if (@available(macOS 14.0, *)) {
            if (s_display_link) {
                [s_display_link invalidate];
                s_display_link = nil;
            }
            s_display_link_proxy = nil;
        }
        app_initialized = FALSE;
    }
}

// Unpaused/started lazily on the first scheduled frame; paused/stopped again
// via ng_macos_frame_idle once the scheduler has nothing pending, so the
// display link doesn't fire while idle.
void ng_macos_request_frame(void) {
    if (s_using_legacy) {
        legacy_display_link_request_frame();
    } else if (@available(macOS 14.0, *)) {
        if (s_display_link) {
            s_display_link.paused = NO;
        }
    }
}

void ng_macos_frame_idle(void) {
    if (s_using_legacy) {
        legacy_display_link_frame_idle();
    } else if (@available(macOS 14.0, *)) {
        if (s_display_link) {
            s_display_link.paused = YES;
        }
    }
}

int ng_macos_run(void) {
    [NSApp run];
    return NG_SUCCESS;
}

int ng_macos_poll_events(void) {
    @autoreleasepool {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, true);
        while (true) {
            NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                              untilDate:[NSDate distantPast]
                                                 inMode:NSDefaultRunLoopMode
                                                dequeue:YES];
            if (event == nil) {
                event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                          untilDate:[NSDate distantPast]
                                             inMode:NSEventTrackingRunLoopMode
                                            dequeue:YES];
            }
            if (event == nil) {
                break;
            }
            [NSApp sendEvent:event];
        }
        [NSApp updateWindows];
    }
    return NG_SUCCESS;
}
