#ifndef AUREA_LINUX_DROP_H
#define AUREA_LINUX_DROP_H

#include <gtk/gtk.h>

/* Files dragged onto `window` report through ng_invoke_file_drop. */
void ng_linux_accept_drops(GtkWidget* window);

#endif
