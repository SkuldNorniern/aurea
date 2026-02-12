#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

extern void ng_invoke_button_callback(unsigned int id);

static void button_clicked(GtkButton *button, gpointer user_data) {
    guint id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(button), "button-id"));
    ng_invoke_button_callback(id);
}

NGHandle ng_linux_create_button(const char* title, unsigned int id) {
    if (!title) return NULL;
    
    GtkWidget* button = gtk_button_new_with_label(title);
    g_object_set_data(G_OBJECT(button), "button-id", GUINT_TO_POINTER(id));
    g_signal_connect(G_OBJECT(button), "clicked", G_CALLBACK(button_clicked), NULL);
    gtk_widget_show(button);
    
    return (NGHandle)button;
}

void ng_linux_button_invalidate(NGHandle button) {
    if (!button) return;
    gtk_widget_queue_draw((GtkWidget*)button);
}

