#import "text.h"

/* Without text input keys are only keys, the view is no text client and nothing composes. */
BOOL ng_macos_text_composing(AureaContentView* view) {
    (void)view;
    return NO;
}

BOOL ng_macos_text_key(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
    return NO;
}

void ng_macos_text_commit(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
}

void ng_macos_text_drop(AureaContentView* view) {
    (void)view;
}
