#include "../elements.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_text_view(int is_editable) {
    return ng_linux_create_text_area(is_editable);
}

void ng_linux_text_view_invalidate(NGHandle text_view) {
    if (!text_view) return;
    gtk_widget_queue_draw((GtkWidget*)text_view);
}
