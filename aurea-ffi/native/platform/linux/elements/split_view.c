#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_split_view(int is_vertical) {
    GtkOrientation orientation = is_vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL;
    GtkWidget* paned = gtk_paned_new(orientation);
    gtk_widget_show(paned);
    return (NGHandle)paned;
}

int ng_linux_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !GTK_IS_PANED(split_handle) || !element || !GTK_IS_WIDGET(element)) {
        return NG_ERROR_INVALID_HANDLE;
    }
    GtkPaned* paned = GTK_PANED(split_handle);
    GtkWidget* widget = (GtkWidget*)element;

    if (gtk_paned_get_child1(paned) && gtk_paned_get_child2(paned)) return NG_ERROR_INVALID_PARAMETER;

    g_object_ref(widget);
    ng_linux_unparent(widget);
    if (!gtk_paned_get_child1(paned)) {
        gtk_paned_pack1(paned, widget, TRUE, TRUE);
    } else {
        gtk_paned_pack2(paned, widget, TRUE, TRUE);
    }
    g_object_unref(widget);
    gtk_widget_show(widget);
    return NG_SUCCESS;
}

int ng_linux_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle || !GTK_IS_PANED(split_handle)) return NG_ERROR_INVALID_HANDLE;
    if (index != 0) return NG_ERROR_INVALID_PARAMETER;
    if (!(position >= 0.0f) || !(position < 1e7f)) return NG_ERROR_INVALID_PARAMETER;
    gtk_paned_set_position(GTK_PANED(split_handle), (int)(position + 0.5f));
    return NG_SUCCESS;
}
