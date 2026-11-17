#import "../elements.h"
#import "../utils.h"
#import <Cocoa/Cocoa.h>

/* Without the widgets the window asks about no control, so every answer is no. */
int ng_macos_is_wrapping_label(NSView* view) {
    (void)view;
    return 0;
}

int ng_macos_text_watch_change(NGHandle element, unsigned int change_id) {
    (void)element;
    (void)change_id;
    return 0;
}

int ng_macos_spin_box_watch_change(NGHandle element, unsigned int id) {
    (void)element;
    (void)id;
    return 0;
}
