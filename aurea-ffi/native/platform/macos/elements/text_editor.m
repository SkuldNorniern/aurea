#import "../elements.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_text_editor(void) {
    return ng_macos_create_text_area(1, 100.0);
}

void ng_macos_text_editor_invalidate(NGHandle text_editor) {
    if (!text_editor) return;
    [(__bridge NSView*)text_editor setNeedsDisplay:YES];
}
