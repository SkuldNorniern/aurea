#ifndef NATIVE_GUI_IOS_UTILS_H
#define NATIVE_GUI_IOS_UTILS_H

#import <Foundation/Foundation.h>

#ifdef __cplusplus
extern "C" {
#endif

// Utility function to convert C string to NSString
NSString* ng_ios_to_nsstring(const char* cstr);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_UTILS_H


