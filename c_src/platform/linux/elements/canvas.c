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

