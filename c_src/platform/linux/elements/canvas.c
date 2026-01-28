#include "../elements.h"
#include "../../common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_canvas(int width, int height) {
    GtkWidget* drawing_area = gtk_drawing_area_new();
    gtk_widget_set_size_request(drawing_area, width, height);
    gtk_widget_show(drawing_area);
    
    return (NGHandle)drawing_area;
}

void ng_linux_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    GtkWidget* widget = (GtkWidget*)canvas;
    cairo_region_t* region = cairo_region_create_rectangle((cairo_rectangle_int_t*)&(cairo_rectangle_int_t){
        .x = (int)x,
        .y = (int)y,
        .width = (int)width,
        .height = (int)height
    });
    gtk_widget_queue_draw_region(widget, region);
    cairo_region_destroy(region);
}

NGHandle ng_linux_canvas_get_window(NGHandle canvas) {
    if (!canvas) return NULL;
    GtkWidget* widget = (GtkWidget*)canvas;
    GtkWindow* window = GTK_WINDOW(gtk_widget_get_toplevel(widget));
    if (GTK_IS_WINDOW(window)) {
        return (NGHandle)window;
    }
    return NULL;
}

NGHandle ng_linux_canvas_get_native_handle(NGHandle canvas) {
    if (!canvas) return NULL;
    GtkWidget* widget = (GtkWidget*)canvas;
    GdkWindow* gdk_window = gtk_widget_get_window(widget);
    if (gdk_window) {
        return (NGHandle)gdk_window;
    }
    return NULL;
}
