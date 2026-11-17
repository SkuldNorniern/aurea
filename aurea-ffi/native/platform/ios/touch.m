#import "touch.h"
#import "host_view.h"

void ng_ios_touch(UIView* view, int action, UITouch* touch) {
    if (!g_ios_host.touch) return;
    CGPoint at = [touch locationInView:view];
    int pencil = touch.type == UITouchTypePencil;
    g_ios_host.touch(action, (float)at.x, (float)at.y, (uintptr_t)(__bridge void*)touch, pencil);
}
