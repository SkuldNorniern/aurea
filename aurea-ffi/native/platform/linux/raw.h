#ifndef AUREA_LINUX_RAW_H
#define AUREA_LINUX_RAW_H

#include <gtk/gtk.h>

/* Relative motion for a window in a raw grab, from a motion event on it. */
void ng_linux_raw_motion(GtkWidget* widget, GdkEventMotion* event);

#endif
