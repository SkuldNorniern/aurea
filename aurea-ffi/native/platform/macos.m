#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import "macos/elements.h"


#import <Cocoa/Cocoa.h>
#import <CoreFoundation/CoreFoundation.h>
#import <CoreVideo/CoreVideo.h>
static BOOL app_initialized = FALSE;

static CVDisplayLinkRef s_display_link = NULL;
static volatile BOOL s_link_running = NO;

@interface AppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}
@end

static AppDelegate* app_delegate = nil;

// CVDisplayLink fires on its own thread; hop to the main queue before
// touching any AppKit/Rust state.
static CVReturn display_link_callback(
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

int ng_macos_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];

        app_delegate = [[AppDelegate alloc] init];
        [NSApp setDelegate:app_delegate];

        [NSApp finishLaunching];
        // Activate once at startup so the first window appears in front.
        [NSApp activateIgnoringOtherApps:YES];

        CVDisplayLinkCreateWithActiveCGDisplays(&s_display_link);
        if (s_display_link) {
            CVDisplayLinkSetOutputCallback(s_display_link, display_link_callback, NULL);
        }

        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_macos_cleanup(void) {
    if (app_initialized) {
        if (s_display_link) {
            if (s_link_running) {
                CVDisplayLinkStop(s_display_link);
                s_link_running = NO;
            }
            CVDisplayLinkRelease(s_display_link);
            s_display_link = NULL;
        }
        app_initialized = FALSE;
    }
}

// Started lazily on the first scheduled frame; stopped via
// ng_macos_frame_idle once the scheduler has nothing pending, so the
// display link (and its main-thread dispatches) don't run while idle.
void ng_macos_request_frame(void) {
    if (s_display_link && !s_link_running) {
        CVDisplayLinkStart(s_display_link);
        s_link_running = YES;
    }
}

void ng_macos_frame_idle(void) {
    if (s_display_link && s_link_running) {
        CVDisplayLinkStop(s_display_link);
        s_link_running = NO;
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
