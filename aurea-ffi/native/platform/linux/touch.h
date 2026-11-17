#ifndef AUREA_LINUX_TOUCH_H
#define AUREA_LINUX_TOUCH_H

#include <gtk/gtk.h>

/* NG_POINTER_TOUCH for a touchscreen source, else 0. */
unsigned int ng_linux_touch_bits(GdkInputSource source);

#endif
