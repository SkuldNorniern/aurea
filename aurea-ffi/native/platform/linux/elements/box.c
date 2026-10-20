#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_box(int is_vertical) {
    GtkOrientation orientation = is_vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL;
    GtkWidget* box = gtk_box_new(orientation, 8);
    gtk_widget_show(box);
    
    return (NGHandle)box;
}

void ng_linux_box_invalidate(NGHandle box) {
    if (!box) return;
    gtk_widget_queue_draw((GtkWidget*)box);
}

int ng_linux_box_add(NGHandle box, NGHandle element, float weight) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;

    gtk_box_pack_start(GTK_BOX(box), (GtkWidget*)element, weight > 0.0f, TRUE, 0);
    gtk_widget_show((GtkWidget*)element);
    
    return NG_SUCCESS;
}
