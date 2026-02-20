#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import "macos/elements.h"


#import <Cocoa/Cocoa.h>
#import <CoreFoundation/CoreFoundation.h>
static BOOL app_initialized = FALSE;

@interface AppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}
@end

static AppDelegate* app_delegate = nil;

int ng_macos_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        
        app_delegate = [[AppDelegate alloc] init];
        [NSApp setDelegate:app_delegate];
        
        [NSApp finishLaunching];
        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_macos_cleanup(void) {
    if (app_initialized) {
        app_initialized = FALSE;
    }
}

// Timer callback to process frames periodically
static void process_frames_timer(CFRunLoopTimerRef timer, void *info) {
    (void)timer;
    (void)info;
    ng_process_frames();
}

int ng_macos_run(void) {
    // Add a timer to process frames periodically (60fps = ~16ms)
    CFRunLoopTimerRef timer = CFRunLoopTimerCreate(
        kCFAllocatorDefault,
        CFAbsoluteTimeGetCurrent(),
        1.0/60.0, // 60fps
        0,
        0,
        process_frames_timer,
        NULL
    );
    if (timer) {
        CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
    }
    
    [NSApp run];
    
    if (timer) {
        CFRunLoopTimerInvalidate(timer);
        CFRelease(timer);
    }
    
    return NG_SUCCESS;
}

int ng_macos_poll_events(void) {
    @autoreleasepool {
        if (![NSApp isActive]) {
            [NSApp activateIgnoringOtherApps:YES];
        }
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
