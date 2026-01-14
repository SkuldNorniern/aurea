#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>
#import <string.h>

int ng_macos_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    
    NSView* view = (__bridge NSView*)text_handle;
    NSString* nsContent = ng_macos_to_nsstring(content);
    
    if ([view isKindOfClass:[NSTextField class]]) {
        [(NSTextField*)view setStringValue:nsContent];
        return NG_SUCCESS;
    }
    
    if ([view isKindOfClass:[NSScrollView class]]) {
        NSScrollView* scrollView = (NSScrollView*)view;
        NSView* docView = [scrollView documentView];
        if ([docView isKindOfClass:[NSTextView class]]) {
            [(NSTextView*)docView setString:nsContent];
            return NG_SUCCESS;
        }
    }
    
    return NG_ERROR_INVALID_HANDLE;
}

char* ng_macos_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    NSView* view = (__bridge NSView*)text_handle;
    NSString* content = nil;
    
    if ([view isKindOfClass:[NSTextField class]]) {
        content = [(NSTextField*)view stringValue];
    } else if ([view isKindOfClass:[NSScrollView class]]) {
        NSScrollView* scrollView = (NSScrollView*)view;
        NSView* docView = [scrollView documentView];
        if ([docView isKindOfClass:[NSTextView class]]) {
            content = [(NSTextView*)docView string];
        }
    }
    
    if (!content) return NULL;
    
    const char* utf8String = [content UTF8String];
    if (!utf8String) return NULL;
    
    return strdup(utf8String);
}

void ng_macos_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}



