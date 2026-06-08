#import "utils.h"
#import <Cocoa/Cocoa.h>

NSString* ng_macos_to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
} 