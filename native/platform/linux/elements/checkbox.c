#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_checkbox(const char* label) {
    GtkWidget* checkbox = gtk_check_button_new_with_label(label ? label : "");
    gtk_widget_show(checkbox);
    
    return (NGHandle)checkbox;
}

int ng_linux_checkbox_set_checked(NGHandle checkbox, int checked) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(checkbox), checked ? TRUE : FALSE);
    return NG_SUCCESS;
}

int ng_linux_checkbox_get_checked(NGHandle checkbox) {
    if (!checkbox) return 0;
    
    return gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(checkbox)) ? 1 : 0;
}

int ng_linux_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    gtk_widget_set_sensitive((GtkWidget*)checkbox, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_checkbox_invalidate(NGHandle checkbox) {
    if (!checkbox) return;
    gtk_widget_queue_draw((GtkWidget*)checkbox);
}



