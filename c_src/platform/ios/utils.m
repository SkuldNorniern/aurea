#import "utils.h"

NSString* ng_ios_to_nsstring(const char* cstr) {
    if (!cstr) return @"";
    return [NSString stringWithUTF8String:cstr];
}




