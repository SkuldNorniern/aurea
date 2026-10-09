#import "utils.h"
#import <Cocoa/Cocoa.h>

NSString* ng_macos_to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
}

/* Accessibility > Display > Reduce motion. */
int ng_macos_reduced_motion(void) {
    return [[NSWorkspace sharedWorkspace] accessibilityDisplayShouldReduceMotion] ? 1 : 0;
}
