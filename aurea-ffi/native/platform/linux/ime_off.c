#include "ime.h"
#include "common/errors.h"

void ng_linux_ime_attach(GtkWidget* window) {
    (void)window;
}

void ng_linux_ime_focus(GtkWidget* window, int focused) {
    (void)window;
    (void)focused;
}

void ng_linux_ime_reset(GtkWidget* window) {
    (void)window;
}

int ng_linux_ime_composing(GtkWidget* window) {
    (void)window;
    return 0;
}

int ng_linux_ime_filter(GtkWidget* window, GdkEventKey* event) {
    (void)window;
    (void)event;
    return 0;
}

int ng_linux_ime_active(GtkWidget* window) {
    (void)window;
    return 0;
}

int ng_linux_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    (void)window;
    (void)x;
    (void)y;
    (void)width;
    (void)height;
    return NG_SUCCESS;
}
