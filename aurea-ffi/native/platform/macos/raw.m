#import "raw.h"
#import "content_view.h"
#import "common/rust_callbacks.h"

void ng_macos_raw_motion(AureaContentView* view, NSEvent* event) {
    if (view.rawSuppress) {
        view.rawSuppress = NO;
        return;
    }
    ng_invoke_raw_mouse_motion(view.windowHandle, (double)[event deltaX], (double)[event deltaY]);
}
