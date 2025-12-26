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

