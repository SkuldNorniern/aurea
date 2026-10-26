#import "utils.h"
#import "../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#include <string.h>
#include <limits.h>

NSString* ng_macos_to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
}

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

void ng_macos_beep(void) {
    NSBeep();
}

int ng_macos_prefers_dark(void) {
    if (@available(macOS 10.14, *)) {
        if (!NSApp) {
            NSString* style = [[NSUserDefaults standardUserDefaults] stringForKey:@"AppleInterfaceStyle"];
            return [style isEqualToString:@"Dark"] ? 1 : 0;
        }
        NSAppearanceName name = [[NSApp effectiveAppearance]
            bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]];
        return [name isEqualToString:NSAppearanceNameDarkAqua] ? 1 : 0;
    }
    return 0;
}

@interface NGAppearanceWatch : NSObject
@property (nonatomic, assign) int dark;
@end

@implementation NGAppearanceWatch
- (void)observeValueForKeyPath:(NSString*)keyPath
                      ofObject:(id)object
                        change:(NSDictionary<NSKeyValueChangeKey, id>*)change
                       context:(void*)context {
    (void)keyPath;
    (void)object;
    (void)change;
    (void)context;
    int dark = ng_macos_prefers_dark();
    if (dark == self.dark) return;
    self.dark = dark;
    ng_invoke_theme_changed(dark);
}
@end

void ng_macos_watch_appearance(void) {
    static NGAppearanceWatch* watch = nil;
    if (watch || !NSApp) return;
    if (@available(macOS 10.14, *)) {
        watch = [[NGAppearanceWatch alloc] init];
        watch.dark = ng_macos_prefers_dark();
        [NSApp addObserver:watch forKeyPath:@"effectiveAppearance" options:0 context:NULL];
    }
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
