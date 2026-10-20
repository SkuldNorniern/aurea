#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_label(const char* text) {
    if (!text) return NULL;
    
    GtkWidget* label = gtk_label_new(text);
    gtk_widget_show(label);
    
    return (NGHandle)label;
}

int ng_linux_label_set_text(NGHandle label, const char* text) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    gtk_label_set_text(GTK_LABEL(label), text);
    return NG_SUCCESS;
}

void ng_linux_label_invalidate(NGHandle label) {
    if (!label) return;
    gtk_widget_queue_draw((GtkWidget*)label);
}

