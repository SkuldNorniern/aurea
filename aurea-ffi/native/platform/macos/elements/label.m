#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_label(const char* text) {
    if (!text) return NULL;
    
    NSTextField* label = [[NSTextField alloc] init];
    [label setStringValue:ng_macos_to_nsstring(text)];
    [label setBezeled:NO];
    [label setDrawsBackground:NO];
    [label setEditable:NO];
    [label setSelectable:NO];
    [label sizeToFit];
    
    return (__bridge_retained void*)label;
}

int ng_macos_label_set_text(NGHandle label, const char* text) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setStringValue:ng_macos_to_nsstring(text)];
    [lbl invalidateIntrinsicContentSize];
    return NG_SUCCESS;
}

void ng_macos_label_invalidate(NGHandle label) {
    if (!label) return;
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setNeedsDisplay:YES];
}

