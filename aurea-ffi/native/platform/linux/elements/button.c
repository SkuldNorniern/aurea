#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

static void button_clicked(GtkButton* button, gpointer user_data) {
    (void)button;
    NgControl* control = user_data;
    ng_invoke_button_callback(control->id);
}

NGHandle ng_linux_create_button(const char* title, unsigned int id) {
    if (!title) return NULL;

    GtkWidget* button = gtk_button_new_with_label(title);
    NgControl* control = ng_control_get(button);
    control->id = id;
    g_signal_connect(button, "clicked", G_CALLBACK(button_clicked), control);
    gtk_widget_show(button);
    return (NGHandle)button;
}

void ng_linux_button_invalidate(NGHandle button) {
    if (!button) return;
    gtk_widget_queue_draw((GtkWidget*)button);
}
