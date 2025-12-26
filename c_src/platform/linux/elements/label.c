#include "../elements.h"
#include "../../common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_label(const char* text) {
    if (!text) return NULL;
    
    GtkWidget* label = gtk_label_new(text);
    gtk_widget_show(label);
    
    return (NGHandle)label;
}

