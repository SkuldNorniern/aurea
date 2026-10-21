#ifndef NATIVE_GUI_MACOS_UTILS_H
#define NATIVE_GUI_MACOS_UTILS_H

#ifdef __OBJC__
@class NSString;
@class NSBitmapImageRep;
#endif

#ifdef __cplusplus
extern "C" {
#endif

#ifdef __OBJC__
NSString* ng_macos_to_nsstring(const char* str);
NSBitmapImageRep* ng_macos_rgba_bitmap(const unsigned char* rgba, unsigned int width, unsigned int height);
#endif

int ng_macos_reduced_motion(void);
int ng_macos_locales(char* buf, int cap);

#ifdef __cplusplus
}
#endif

#endif