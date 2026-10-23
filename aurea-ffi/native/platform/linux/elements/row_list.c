#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

#define ROW_LIST_DATA_KEY "aurea-row-list"

typedef struct {
    unsigned int id;
    GtkListBox* box;
    int selected;
} RowListData;

static RowListData* list_data(NGHandle list) {
    if (!list || !GTK_IS_SCROLLED_WINDOW(list)) return NULL;
    return (RowListData*)g_object_get_data(G_OBJECT(list), ROW_LIST_DATA_KEY);
}

static int selected_index(RowListData* data) {
    GtkListBoxRow* row = gtk_list_box_get_selected_row(data->box);
    return row ? gtk_list_box_row_get_index(row) : -1;
}

static void on_row_selected(GtkListBox* box, GtkListBoxRow* row, gpointer user_data) {
    (void)row;
    RowListData* data = (RowListData*)user_data;
    if (ng_linux_is_quiet(box)) return;
    int index = selected_index(data);
    if (index != data->selected) {
        data->selected = index;
        ng_invoke_row_list_selected(data->id, index);
    }
}

static void on_row_activated(GtkListBox* box, GtkListBoxRow* row, gpointer user_data) {
    (void)box;
    RowListData* data = (RowListData*)user_data;
    ng_invoke_row_list_activated(data->id, gtk_list_box_row_get_index(row));
}

NGHandle ng_linux_create_row_list(unsigned int id) {
    GtkWidget* scroll = gtk_scrolled_window_new(NULL, NULL);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll), GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_shadow_type(GTK_SCROLLED_WINDOW(scroll), GTK_SHADOW_IN);
    gtk_scrolled_window_set_min_content_height(GTK_SCROLLED_WINDOW(scroll), 120);
    GtkWidget* box = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(box), GTK_SELECTION_SINGLE);
    gtk_list_box_set_activate_on_single_click(GTK_LIST_BOX(box), FALSE);
    gtk_container_add(GTK_CONTAINER(scroll), box);

    RowListData* data = g_new0(RowListData, 1);
    data->id = id;
    data->box = GTK_LIST_BOX(box);
    data->selected = -1;
    g_object_set_data_full(G_OBJECT(scroll), ROW_LIST_DATA_KEY, data, g_free);
    g_signal_connect(box, "row-selected", G_CALLBACK(on_row_selected), data);
    g_signal_connect(box, "row-activated", G_CALLBACK(on_row_activated), data);
    gtk_widget_show_all(scroll);
    return (NGHandle)scroll;
}

static int row_count(RowListData* data) {
    GList* rows = gtk_container_get_children(GTK_CONTAINER(data->box));
    int count = (int)g_list_length(rows);
    g_list_free(rows);
    return count;
}

/* Changes made by the app are not reported. */
static void quiet(RowListData* data, int on) {
    ng_linux_set_quiet(data->box, on);
    if (!on) data->selected = selected_index(data);
}

int ng_linux_row_list_insert(NGHandle list, int index, NGHandle element) {
    RowListData* data = list_data(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (!element || !GTK_IS_WIDGET(element) || index < 0 || index > row_count(data)) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    GtkWidget* widget = (GtkWidget*)element;
    GtkWidget* old = gtk_widget_get_parent(widget);
    if (old) {
        g_object_ref(widget);
        gtk_container_remove(GTK_CONTAINER(old), widget);
    }
    quiet(data, 1);
    gtk_list_box_insert(data->box, widget, index);
    quiet(data, 0);
    if (old) g_object_unref(widget);
    gtk_widget_show(widget);
    gtk_widget_show(gtk_widget_get_parent(widget));
    return NG_SUCCESS;
}

int ng_linux_row_list_remove(NGHandle list, int index) {
    RowListData* data = list_data(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    GtkListBoxRow* row = gtk_list_box_get_row_at_index(data->box, index);
    if (!row) return NG_ERROR_INVALID_PARAMETER;
    quiet(data, 1);
    gtk_widget_destroy(GTK_WIDGET(row));
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_clear(NGHandle list) {
    RowListData* data = list_data(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    quiet(data, 1);
    GList* rows = gtk_container_get_children(GTK_CONTAINER(data->box));
    for (GList* at = rows; at; at = at->next) gtk_widget_destroy(GTK_WIDGET(at->data));
    g_list_free(rows);
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_set_selected(NGHandle list, int index) {
    RowListData* data = list_data(list);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    quiet(data, 1);
    if (index < 0) {
        gtk_list_box_unselect_all(data->box);
    } else {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(data->box, index);
        gtk_list_box_select_row(data->box, row);
        GtkAdjustment* v = gtk_scrolled_window_get_vadjustment(GTK_SCROLLED_WINDOW(list));
        GtkAllocation at;
        gtk_widget_get_allocation(GTK_WIDGET(row), &at);
        if (at.height > 0) gtk_adjustment_clamp_page(v, at.y, at.y + at.height);
    }
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_get_selected(NGHandle list) {
    RowListData* data = list_data(list);
    return data ? selected_index(data) : -1;
}
