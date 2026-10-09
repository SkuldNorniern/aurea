#ifndef NATIVE_GUI_MACOS_UTILS_H
#define NATIVE_GUI_MACOS_UTILS_H

#ifdef __OBJC__
@class NSString;
#endif

#ifdef __cplusplus
extern "C" {
#endif

#ifdef __OBJC__
NSString* ng_macos_to_nsstring(const char* str);
#endif

int ng_macos_reduced_motion(void);

#ifdef __cplusplus
}
#endif

#endif