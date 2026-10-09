#ifndef AUREA_LINUX_IME_H
#define AUREA_LINUX_IME_H

#include "common/types.h"
#include <gtk/gtk.h>

/* Gives a toplevel its input method context. Once, at creation. */
void ng_linux_ime_attach(GtkWidget* window);

void ng_linux_ime_focus(GtkWidget* window, int focused);

/* Whether the input method is in the middle of composing text. */
int ng_linux_ime_composing(GtkWidget* window);

/* Offers a key press or release to the input method; 1 if it took it. */
int ng_linux_ime_filter(GtkWidget* window, GdkEventKey* event);

int ng_linux_ime_available(GtkWidget* window);

int ng_linux_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height);

#endif
