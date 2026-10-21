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
#import <stdatomic.h>
static BOOL app_initialized = FALSE;
static BOOL s_in_nsapp_run = NO;

@interface AppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    (void)sender;
    return s_in_nsapp_run && ng_macos_hide_on_close_count == 0;
}

- (NSApplicationTerminateReply)applicationShouldTerminate:(NSApplication *)sender {
    if (!s_in_nsapp_run) return NSTerminateNow;
    [sender stop:nil];
    NSEvent* nudge = [NSEvent otherEventWithType:NSEventTypeApplicationDefined
                                        location:NSZeroPoint
                                   modifierFlags:0
                                       timestamp:0
                                    windowNumber:0
                                         context:nil
                                         subtype:0
                                           data1:0
                                           data2:0];
    [sender postEvent:nudge atStart:NO];
    return NSTerminateCancel;
}
@end

static AppDelegate* app_delegate = nil;

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

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"

static CVDisplayLinkRef s_cv_display_link = NULL;
static atomic_bool s_cv_frame_queued = false;
static volatile BOOL s_cv_link_running = NO;
static BOOL s_using_legacy = NO;

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
    if (atomic_exchange(&s_cv_frame_queued, true)) return kCVReturnSuccess;
    dispatch_async(dispatch_get_main_queue(), ^{
        atomic_store(&s_cv_frame_queued, false);
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
        [NSEvent setMouseCoalescingEnabled:NO];

        app_delegate = [[AppDelegate alloc] init];
        [NSApp setDelegate:app_delegate];

        [NSApp finishLaunching];
        [NSApp activateIgnoringOtherApps:YES];

        if (@available(macOS 14.0, *)) {
            s_display_link_proxy = [[NgDisplayLinkProxy alloc] init];
            NSScreen* screen = [NSScreen mainScreen];
            if (screen) {
                s_display_link = [screen displayLinkWithTarget:s_display_link_proxy
                                                        selector:@selector(onDisplayLink:)];
                s_display_link.paused = YES;
                [s_display_link addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
            } else {
                legacy_display_link_init();
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

static atomic_int s_waiting = 0;
static const short AUREA_WAKE_SUBTYPE = 0x4157;

static void ng_macos_wake_wait(void) {
    if (!atomic_exchange(&s_waiting, 0)) return;
    NSEvent* wake = [NSEvent otherEventWithType:NSEventTypeApplicationDefined
                                       location:NSZeroPoint
                                  modifierFlags:0
                                      timestamp:0
                                   windowNumber:0
                                        context:nil
                                        subtype:AUREA_WAKE_SUBTYPE
                                          data1:0
                                          data2:0];
    [NSApp postEvent:wake atStart:NO];
}
static void request_frame_on_main(void) {
    if (s_using_legacy) {
        legacy_display_link_request_frame();
    } else if (@available(macOS 14.0, *)) {
        if (s_display_link) {
            s_display_link.paused = NO;
        }
    }
}

void ng_macos_request_frame(void) {
    ng_macos_wake_wait();
    if ([NSThread isMainThread]) {
        request_frame_on_main();
    } else {
        dispatch_async(dispatch_get_main_queue(), ^{
            request_frame_on_main();
        });
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
    s_in_nsapp_run = YES;
    [NSApp run];
    s_in_nsapp_run = NO;
    return NG_SUCCESS;
}

int ng_macos_poll_events(void) {
    @autoreleasepool {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, true);
        CFAbsoluteTime until = CFAbsoluteTimeGetCurrent() + 0.002;
        for (int handled = 0; handled < 64 && CFAbsoluteTimeGetCurrent() < until; handled++) {
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

int ng_macos_wait_events(int timeout_ms) {
    if (timeout_ms == 0) return ng_macos_poll_events();
    @autoreleasepool {
        NSDate* until = timeout_ms < 0
            ? [NSDate distantFuture]
            : [NSDate dateWithTimeIntervalSinceNow:timeout_ms / 1000.0];
        atomic_store(&s_waiting, 1);
        NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                            untilDate:until
                                               inMode:NSDefaultRunLoopMode
                                              dequeue:YES];
        atomic_store(&s_waiting, 0);
        BOOL wake = event && event.type == NSEventTypeApplicationDefined
            && event.subtype == AUREA_WAKE_SUBTYPE;
        if (event && !wake) [NSApp sendEvent:event];
    }
    return ng_macos_poll_events();
}
