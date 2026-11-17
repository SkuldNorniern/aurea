#include "raw.h"
#include "window_state.h"
#include "common/rust_callbacks.h"

void ng_linux_raw_motion(GtkWidget* widget, GdkEventMotion* event) {
    NgLinuxWindow* w = ng_linux_window_state(widget);
    if (w->grab_mode == 2) {
        if (w->mouse_known) {
            ng_invoke_raw_mouse_motion((void*)widget, event->x - w->mouse_x, event->y - w->mouse_y);
        }
        w->mouse_known = TRUE;
        w->mouse_x = event->x;
        w->mouse_y = event->y;
    }
}
