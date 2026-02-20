#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_split_view(int is_vertical) {
    GtkOrientation orientation = is_vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL;
    GtkWidget* paned = gtk_paned_new(orientation);
    gtk_widget_show(paned);
    return (NGHandle)paned;
}

int ng_linux_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !element) return NG_ERROR_INVALID_HANDLE;

    GtkPaned* paned = GTK_PANED(split_handle);
    GtkWidget* widget = (GtkWidget*)element;

    GtkWidget* child1 = gtk_paned_get_child1(paned);
    GtkWidget* child2 = gtk_paned_get_child2(paned);

    if (!child1) {
        gtk_paned_pack1(paned, widget, TRUE, TRUE);
    } else if (!child2) {
        gtk_paned_pack2(paned, widget, TRUE, TRUE);
    } else {
        return NG_ERROR_INVALID_PARAMETER;
    }

    gtk_widget_show(widget);
    return NG_SUCCESS;
}

int ng_linux_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    if (index != 0) return NG_ERROR_INVALID_PARAMETER;

    GtkPaned* paned = GTK_PANED(split_handle);
    gtk_paned_set_position(paned, (int)position);
    return NG_SUCCESS;
}
