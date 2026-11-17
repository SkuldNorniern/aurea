#import "raw.h"

/* Without raw mouse a locked cursor only stays put. */
void ng_macos_raw_motion(AureaContentView* view, NSEvent* event) {
    (void)view;
    (void)event;
}
