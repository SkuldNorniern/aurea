#include "../elements.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_text_editor(void) {
    return ng_linux_create_text_area(1);
}

void ng_linux_text_editor_invalidate(NGHandle text_editor) {
    if (!text_editor) return;
    gtk_widget_queue_draw((GtkWidget*)text_editor);
}
