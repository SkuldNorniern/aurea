#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

static void on_destroyed(GtkWidget* widget, gpointer id) {
    (void)widget;
    ng_invoke_custom_callback(GPOINTER_TO_UINT(id));
}

int ng_linux_element_set_change_id(NGHandle element, unsigned int id) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* child = GTK_IS_BIN(element) ? gtk_bin_get_child(GTK_BIN(element)) : NULL;
    if (GTK_IS_TEXT_VIEW(child)) {
        ng_control_watch(gtk_text_view_get_buffer(GTK_TEXT_VIEW(child)), "changed", id);
        return NG_SUCCESS;
    }
    const char* signal = GTK_IS_ENTRY(element)         ? "changed"
                       : GTK_IS_TOGGLE_BUTTON(element) ? "toggled"
                       : GTK_IS_COMBO_BOX(element)     ? "changed"
                       : GTK_IS_RANGE(element)         ? "value-changed"
                                                       : NULL;
    if (!signal) return NG_ERROR_UNSUPPORTED;
    ng_control_watch(element, signal, id);
    return NG_SUCCESS;
}

int ng_linux_element_set_enabled(NGHandle element, int enabled) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_set_sensitive((GtkWidget*)element, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

int ng_linux_element_watch_destroy(NGHandle element, unsigned int id) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    g_signal_connect(element, "destroy", G_CALLBACK(on_destroyed), GUINT_TO_POINTER(id));
    return NG_SUCCESS;
}

int ng_linux_element_set_visible(NGHandle element, int visible) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_set_visible((GtkWidget*)element, visible ? TRUE : FALSE);
    return NG_SUCCESS;
}

int ng_linux_element_set_min_size(NGHandle element, int width, int height) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_set_size_request((GtkWidget*)element, width ? width : -1, height ? height : -1);
    return NG_SUCCESS;
}

int ng_linux_element_set_accessible_name(NGHandle element, const char* name) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    if (!name) return NG_ERROR_INVALID_PARAMETER;
    GtkWidget* widget = (GtkWidget*)element;
    GtkWidget* inner = GTK_IS_SCROLLED_WINDOW(widget) ? gtk_bin_get_child(GTK_BIN(widget)) : NULL;
    if (inner && GTK_IS_VIEWPORT(inner)) inner = gtk_bin_get_child(GTK_BIN(inner));
    if (inner && (GTK_IS_TEXT_VIEW(inner) || GTK_IS_TREE_VIEW(inner) || GTK_IS_LIST_BOX(inner))) widget = inner;
    atk_object_set_name(gtk_widget_get_accessible(widget), name);
    return NG_SUCCESS;
}

int ng_linux_element_set_tooltip(NGHandle element, const char* text) {
    if (!element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_set_tooltip_text((GtkWidget*)element, *text ? text : NULL);
    return NG_SUCCESS;
}

int ng_linux_box_set_spacing(NGHandle box, int spacing) {
    if (!box || !GTK_IS_BOX(box)) return NG_ERROR_INVALID_HANDLE;
    if (spacing < 0) return NG_ERROR_INVALID_PARAMETER;
    gtk_box_set_spacing(GTK_BOX(box), spacing);
    return NG_SUCCESS;
}

int ng_linux_box_set_padding(NGHandle box, int padding) {
    if (!box || !GTK_IS_BOX(box)) return NG_ERROR_INVALID_HANDLE;
    if (padding < 0) return NG_ERROR_INVALID_PARAMETER;
    gtk_container_set_border_width(GTK_CONTAINER(box), (guint)padding);
    return NG_SUCCESS;
}

NGHandle ng_linux_create_box(int is_vertical) {
    GtkOrientation orientation = is_vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL;
    GtkWidget* box = gtk_box_new(orientation, 8);
    gtk_widget_show(box);
    return (NGHandle)box;
}

void ng_linux_box_invalidate(NGHandle box) {
    if (!box) return;
    gtk_widget_queue_draw((GtkWidget*)box);
}

/* An element that already has a parent moves here; GTK would refuse it and report success. */
int ng_linux_box_add(NGHandle box, NGHandle element, float weight) {
    if (!box || !GTK_IS_BOX(box) || !element || !GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* child = (GtkWidget*)element;
    g_object_ref(child);
    ng_linux_unparent(child);
    gtk_box_pack_start(GTK_BOX(box), child, weight > 0.0f, TRUE, 0);
    g_object_unref(child);
    gtk_widget_show(child);
    return NG_SUCCESS;
}
