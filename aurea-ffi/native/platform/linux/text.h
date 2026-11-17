#ifndef AUREA_LINUX_TEXT_H
#define AUREA_LINUX_TEXT_H

#include <gtk/gtk.h>

/* Reports the text of a key press, unless the input method made it. */
void ng_linux_text_key(GtkWidget* widget, GdkEventKey* event);

#endif
