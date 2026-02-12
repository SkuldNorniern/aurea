#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_label(const char* text) {
    if (!text) return NULL;
    
    GtkWidget* label = gtk_label_new(text);
    gtk_widget_show(label);
    
    return (NGHandle)label;
}

void ng_linux_label_invalidate(NGHandle label) {
    if (!label) return;
    gtk_widget_queue_draw((GtkWidget*)label);
}

