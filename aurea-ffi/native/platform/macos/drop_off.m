#import "drop.h"

/* Without file drops the view takes no drag, these never run. */
void ng_macos_drop_register(AureaContentView* view) {
    (void)view;
}

NSDragOperation ng_macos_drop_hover(AureaContentView* view, id<NSDraggingInfo> info) {
    (void)view;
    (void)info;
    return NSDragOperationNone;
}

void ng_macos_drop_leave(AureaContentView* view) {
    (void)view;
}

BOOL ng_macos_drop_perform(AureaContentView* view, id<NSDraggingInfo> info) {
    (void)view;
    (void)info;
    return NO;
}
