#import "utils.h"
#import <Cocoa/Cocoa.h>
#include <string.h>
#include <limits.h>

NSString* ng_macos_to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
}

/* Straight RGBA8 pixels from the app, tagged sRGB so they look the same on a
   wide-gamut display as everywhere else. */
NSBitmapImageRep* ng_macos_rgba_bitmap(const unsigned char* rgba, unsigned int width, unsigned int height) {
    if (!rgba || width == 0 || height == 0 || width > INT_MAX / 4 || height > INT_MAX) return nil;
    NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc]
        initWithBitmapDataPlanes:NULL
                      pixelsWide:(NSInteger)width
                      pixelsHigh:(NSInteger)height
                   bitsPerSample:8
                 samplesPerPixel:4
                        hasAlpha:YES
                        isPlanar:NO
                  colorSpaceName:NSDeviceRGBColorSpace
                    bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
                     bytesPerRow:(NSInteger)width * 4
                    bitsPerPixel:32];
    if (!bitmap || !bitmap.bitmapData) return nil;
    memcpy(bitmap.bitmapData, rgba, (size_t)width * height * 4);
    return [bitmap bitmapImageRepByRetaggingWithColorSpace:[NSColorSpace sRGBColorSpace]] ?: bitmap;
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
