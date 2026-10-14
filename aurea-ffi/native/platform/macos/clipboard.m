#import <Cocoa/Cocoa.h>
#import <ImageIO/ImageIO.h>
#include "common/clipboard.h"

static NSString* pasteboard_type(const char* format) {
    if (!strcmp(format, "image/png")) return NSPasteboardTypePNG;
    return [NSString stringWithUTF8String:format];
}
int ng_macos_clipboard_write(const NGClipboardEntry* entries, size_t count) {
    @autoreleasepool {
        if (!count) { [[NSPasteboard generalPasteboard] clearContents]; return NG_SUCCESS; }
        NSPasteboardItem* item = [[NSPasteboardItem alloc] init];
        int result = NG_SUCCESS;
        for (size_t i = 0; i < count; ++i) {
            NSString* type = entries[i].format ? pasteboard_type(entries[i].format) : nil;
            if (!type || (entries[i].len && !entries[i].data)) { result = NG_ERROR_INVALID_PARAMETER; break; }
            for (size_t k = 0; k < i; ++k) if ([type isEqualToString:pasteboard_type(entries[k].format)])
                result = NG_ERROR_INVALID_PARAMETER;
            if (result) break;
            NSData* bytes = [NSData dataWithBytes:entries[i].data length:entries[i].len];
            if (![item setData:bytes forType:type]) { result = NG_ERROR_PLATFORM_SPECIFIC; break; }
        }
        if (!result) {
            NSPasteboard* clipboard = [NSPasteboard generalPasteboard];
            [clipboard clearContents];
            if (![clipboard writeObjects:@[item]]) result = NG_ERROR_PLATFORM_SPECIFIC;
        }
        [item release];
        return result;
    }
}

/* Inspect TIFF dimensions before asking ImageIO to allocate decoded pixels. */
static NSData* png_from_tiff(NSData* tiff, size_t max_bytes, int* result) {
    if (!tiff || tiff.length > max_bytes) { *result = NG_ERROR_LIMIT_EXCEEDED; return nil; }
    CGImageSourceRef source = CGImageSourceCreateWithData((CFDataRef)tiff, NULL);
    if (!source) { *result = NG_ERROR_INVALID_PARAMETER; return nil; }
    NSDictionary* properties = (NSDictionary*)CGImageSourceCopyPropertiesAtIndex(source, 0, NULL);
    unsigned long long width = [[properties objectForKey:(NSString*)kCGImagePropertyPixelWidth] unsignedLongLongValue];
    unsigned long long height = [[properties objectForKey:(NSString*)kCGImagePropertyPixelHeight] unsignedLongLongValue];
    [properties release];
    if (!width || !height) { *result = NG_ERROR_INVALID_PARAMETER; CFRelease(source); return nil; }
    if (width > SIZE_MAX / 4 || height > max_bytes / (width * 4)) {
        *result = NG_ERROR_LIMIT_EXCEEDED; CFRelease(source); return nil;
    }
    CGImageRef image = CGImageSourceCreateImageAtIndex(source, 0, NULL);
    CFRelease(source);
    if (!image) { *result = NG_ERROR_PLATFORM_SPECIFIC; return nil; }
    NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc] initWithCGImage:image];
    CGImageRelease(image);
    NSData* png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
    [bitmap release];
    if (!png) *result = NG_ERROR_PLATFORM_SPECIFIC;
    return png;
}

int ng_macos_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len) {
    @autoreleasepool {
        NSString* type = pasteboard_type(format);
        if (!type) return NG_ERROR_INVALID_PARAMETER;
        NSPasteboard* clipboard = [NSPasteboard generalPasteboard];
        NSData* bytes = [clipboard dataForType:type];
        if (!bytes && !strcmp(format, "image/png")) {
            NSData* tiff = [clipboard dataForType:NSPasteboardTypeTIFF];
            if (tiff) {
                int result = NG_SUCCESS;
                bytes = png_from_tiff(tiff, max_bytes, &result);
                if (result) return result;
            }
        }
        if (!bytes) return NG_SUCCESS;
        if (bytes.length > max_bytes) return NG_ERROR_LIMIT_EXCEEDED;
        unsigned char* copy = (unsigned char*)malloc(bytes.length ? bytes.length : 1);
        if (!copy) return NG_ERROR_PLATFORM_SPECIFIC;
        if (bytes.length) memcpy(copy, bytes.bytes, bytes.length);
        *data = copy; *len = bytes.length;
        return NG_SUCCESS;
    }
}
int ng_macos_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len) {
    @autoreleasepool {
        int result = NG_SUCCESS;
        for (NSString* type in [[NSPasteboard generalPasteboard] types]) {
            const char* name = [type isEqualToString:NSPasteboardTypePNG] ? "image/png" : type.UTF8String;
            if (name && (result = ng_clipboard_append(data, len, name, max_bytes))) break;
        }
        return result;
    }
}
