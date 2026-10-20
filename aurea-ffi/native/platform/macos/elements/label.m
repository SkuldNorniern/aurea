#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/platform_api.h"
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

int ng_macos_label_set_style(NGHandle label, int style) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (style < NG_TEXT_STYLE_BODY || style > NG_TEXT_STYLE_HEADING) return NG_ERROR_INVALID_PARAMETER;
    CGFloat size = [NSFont systemFontSize];
    NSFont* font = style == NG_TEXT_STYLE_STRONG ? [NSFont systemFontOfSize:size weight:NSFontWeightSemibold]
                 : style == NG_TEXT_STYLE_SMALL ? [NSFont systemFontOfSize:[NSFont smallSystemFontSize]]
                 : style == NG_TEXT_STYLE_HEADING ? [NSFont systemFontOfSize:size * 1.4 weight:NSFontWeightSemibold]
                                                  : [NSFont systemFontOfSize:size];
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setFont:font];
    [lbl invalidateIntrinsicContentSize];
    return NG_SUCCESS;
}

void ng_macos_label_invalidate(NGHandle label) {
    if (!label) return;
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setNeedsDisplay:YES];
}

