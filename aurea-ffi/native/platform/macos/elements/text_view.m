#import "../elements.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_text_view(int is_editable) {
    return ng_macos_create_text_area(is_editable, is_editable ? 28.0 : 24.0);
}

void ng_macos_text_view_invalidate(NGHandle text_view) {
    if (!text_view) return;
    [(__bridge NSView*)text_view setNeedsDisplay:YES];
}
