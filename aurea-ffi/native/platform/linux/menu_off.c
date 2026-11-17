#include "menu.h"

int ng_linux_menu_key_press(GtkWidget* widget, GdkEventKey* event, unsigned int keycode, unsigned int mods) {
    (void)widget;
    (void)event;
    (void)keycode;
    (void)mods;
    return 0;
}

int ng_linux_menu_key_release(GtkWidget* widget, GdkEventKey* event) {
    (void)widget;
    (void)event;
    return 0;
}

void ng_linux_menu_window_destroyed(GtkWidget* widget) {
    (void)widget;
}
