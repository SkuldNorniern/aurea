#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

typedef struct {
    unsigned int id;
    GPtrArray* buttons;
    int selected;
    int updating;
} TabBarData;

static void tabbar_data_free(gpointer p) {
    TabBarData* data = (TabBarData*)p;
    if (data->buttons)
        g_ptr_array_free(data->buttons, TRUE);
    g_free(data);
}

static TabBarData* tab_data(NGHandle tab_bar) {
    return tab_bar ? g_object_get_data(G_OBJECT(tab_bar), "tabbar-data") : NULL;
}

static void show_selected(TabBarData* data) {
    data->updating = 1;
    for (guint i = 0; i < data->buttons->len; i++) {
        GtkWidget* btn = g_ptr_array_index(data->buttons, i);
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(btn), (int)i == data->selected);
    }
    data->updating = 0;
}

static void on_tab_clicked(GtkButton* btn, gpointer user_data) {
    TabBarData* data = (TabBarData*)user_data;
    if (data->updating) return;
    int idx = GPOINTER_TO_INT(g_object_get_data(G_OBJECT(btn), "tab-index"));
    int changed = idx != data->selected;
    data->selected = idx;
    show_selected(data);
    if (changed) ng_invoke_tab_bar_selected(data->id, idx);
}

NGHandle ng_linux_create_tab_bar(unsigned int id) {
    TabBarData* data = g_new0(TabBarData, 1);
    data->id = id;
    data->selected = -1;
    data->buttons = g_ptr_array_new_with_free_func((GDestroyNotify)g_object_unref);
    GtkWidget* box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);
    g_object_set_data_full(G_OBJECT(box), "tabbar-data", data, tabbar_data_free);
    gtk_widget_show(box);
    return (NGHandle)box;
}

int ng_linux_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;
    TabBarData* data = tab_data(tab_bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* btn = gtk_toggle_button_new_with_label(title);
    int idx = data->buttons->len;
    g_object_set_data(G_OBJECT(btn), "tab-index", GINT_TO_POINTER(idx));
    g_signal_connect(btn, "clicked", G_CALLBACK(on_tab_clicked), data);
    gtk_box_pack_start(GTK_BOX(tab_bar), btn, FALSE, FALSE, 2);
    gtk_widget_show(btn);
    g_ptr_array_add(data->buttons, g_object_ref_sink(btn));
    if (data->selected < 0) data->selected = 0;
    show_selected(data);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    TabBarData* data = tab_data(tab_bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)data->buttons->len) return NG_ERROR_INVALID_PARAMETER;
    gtk_container_remove(GTK_CONTAINER(tab_bar), g_ptr_array_index(data->buttons, index));
    g_ptr_array_remove_index(data->buttons, index);
    for (guint i = index; i < data->buttons->len; i++) {
        g_object_set_data(G_OBJECT(g_ptr_array_index(data->buttons, i)), "tab-index", GINT_TO_POINTER(i));
    }
    int count = (int)data->buttons->len;
    if (data->selected > index || data->selected >= count) data->selected--;
    if (data->selected < 0 && count > 0) data->selected = 0;
    show_selected(data);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_set_selected(NGHandle tab_bar, int index) {
    TabBarData* data = tab_data(tab_bar);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)data->buttons->len) return NG_ERROR_INVALID_PARAMETER;
    data->selected = index;
    show_selected(data);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_get_selected(NGHandle tab_bar) {
    TabBarData* data = tab_data(tab_bar);
    return data ? data->selected : -1;
}

void ng_linux_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    gtk_widget_queue_draw((GtkWidget*)tab_bar);
}
