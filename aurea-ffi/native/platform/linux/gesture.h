#ifndef AUREA_LINUX_GESTURE_H
#define AUREA_LINUX_GESTURE_H

#include <gtk/gtk.h>

/* Zoom and rotate gestures of a window, kept as long as the window. */
void ng_linux_connect_gestures(GtkWidget* window);

#endif
