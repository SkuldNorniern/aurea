#import "elements.h"
#import "utils.h"
#import "../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_button(const char* title) {
    if (!title) return NULL;
    
    NSButton* button = [[NSButton alloc] init];
    [button setTitle:ng_macos_to_nsstring(title)];
    [button setBezelStyle:NSBezelStyleRounded];
    [button sizeToFit];
    
    return (__bridge_retained void*)button;
}

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

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:8.0];
    [stack setAlignment:NSLayoutAttributeCenterY];
    
    return (__bridge_retained void*)stack;
}

int ng_macos_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSStackView* stack = (__bridge NSStackView*)box;
    NSView* view = (__bridge NSView*)element;
    
    [stack addArrangedSubview:view];
    
    return NG_SUCCESS;
} 