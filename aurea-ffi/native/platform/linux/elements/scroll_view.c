#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_scroll_view(void) {
    GtkWidget* scroll = gtk_scrolled_window_new(NULL, NULL);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll), GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_propagate_natural_width(GTK_SCROLLED_WINDOW(scroll), TRUE);
    gtk_widget_show(scroll);
    return (NGHandle)scroll;
}

int ng_linux_scroll_view_set_content(NGHandle scroll_handle, NGHandle element) {
    if (!scroll_handle || !GTK_IS_SCROLLED_WINDOW(scroll_handle) || !element || !GTK_IS_WIDGET(element)) {
        return NG_ERROR_INVALID_HANDLE;
    }
    GtkContainer* scroll = GTK_CONTAINER(scroll_handle);
    GtkWidget* content = (GtkWidget*)element;
    GtkWidget* old = gtk_bin_get_child(GTK_BIN(scroll));
    if (old == content || (GTK_IS_VIEWPORT(old) && gtk_bin_get_child(GTK_BIN(old)) == content)) {
        return NG_SUCCESS;
    }
    if (old) gtk_container_remove(scroll, old);
    gtk_container_add(scroll, content);
    if (GTK_IS_CONTAINER(content)) {
        GtkAdjustment* v = gtk_scrolled_window_get_vadjustment(GTK_SCROLLED_WINDOW(scroll));
        gtk_container_set_focus_vadjustment(GTK_CONTAINER(content), v);
    }
    gtk_widget_show(content);
    return NG_SUCCESS;
}
