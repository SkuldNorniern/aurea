#ifndef AUREA_LINUX_PEN_H
#define AUREA_LINUX_PEN_H

#include <gtk/gtk.h>

enum { PEN_HOVER = 0, PEN_DOWN = 1, PEN_MOVE = 2, PEN_UP = 3 };

/* Reports a pen or eraser sample of `event` at `x`, `y`; `phase` is one of PEN_*. */
void ng_linux_pen_report(GtkWidget* widget, GdkEvent* event, double x, double y, guint state, unsigned int phase);
/* NG_POINTER_PEN for a pen or eraser source, else 0. */
unsigned int ng_linux_pen_bits(GdkInputSource source);

#endif
