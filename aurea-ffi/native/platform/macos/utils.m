#import "utils.h"
#import <Cocoa/Cocoa.h>
#include <string.h>

NSString* ng_macos_to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
}

int ng_macos_reduced_motion(void) {
    return [[NSWorkspace sharedWorkspace] accessibilityDisplayShouldReduceMotion] ? 1 : 0;
}

int ng_macos_locales(char* buf, int cap) {
    int need = 0;
    for (NSString* tag in [NSLocale preferredLanguages]) {
        need += (int)strlen(tag.UTF8String) + 1;
    }
    if (buf && need > 0 && cap >= need) {
        char* at = buf;
        for (NSString* tag in [NSLocale preferredLanguages]) {
            size_t n = strlen(tag.UTF8String) + 1;
            memcpy(at, tag.UTF8String, n);
            at += n;
        }
    }
    return need;
}
