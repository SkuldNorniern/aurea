#include "pen.h"

void ng_linux_pen_report(GtkWidget* widget, GdkEvent* event, double x, double y, guint state, unsigned int phase) {
    (void)widget;
    (void)event;
    (void)x;
    (void)y;
    (void)state;
    (void)phase;
}

unsigned int ng_linux_pen_bits(GdkInputSource source) {
    (void)source;
    return 0;
}
