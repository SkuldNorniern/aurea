#include "../elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

#define TABLE_DATA_KEY "aurea-table"

typedef struct {
    unsigned int id;
    GtkTreeView* view;
    GtkListStore* store;
    int columns;
    int selected;
} TableData;

static TableData* table_data(NGHandle table) {
    if (!table || !GTK_IS_SCROLLED_WINDOW(table)) return NULL;
    return (TableData*)g_object_get_data(G_OBJECT(table), TABLE_DATA_KEY);
}

static void table_data_free(gpointer p) {
    TableData* data = (TableData*)p;
    if (data->store) g_object_unref(data->store);
    g_free(data);
}

static int view_selected(TableData* data) {
    GtkTreeModel* model = NULL;
    GtkTreeIter iter;
    if (!gtk_tree_selection_get_selected(gtk_tree_view_get_selection(data->view), &model, &iter)) return -1;
    GtkTreePath* path = gtk_tree_model_get_path(model, &iter);
    int row = gtk_tree_path_get_indices(path)[0];
    gtk_tree_path_free(path);
    return row;
}

static void on_selection_changed(GtkTreeSelection* selection, gpointer user_data) {
    TableData* data = (TableData*)user_data;
    if (ng_linux_is_quiet(selection)) return;
    int row = view_selected(data);
    if (row != data->selected) {
        data->selected = row;
        ng_invoke_table_selected(data->id, row);
    }
}

static void on_row_activated(GtkTreeView* view, GtkTreePath* path, GtkTreeViewColumn* column,
                             gpointer user_data) {
    (void)view;
    (void)column;
    TableData* data = (TableData*)user_data;
    ng_invoke_table_activated(data->id, gtk_tree_path_get_indices(path)[0]);
}

NGHandle ng_linux_create_table(unsigned int id) {
    GtkWidget* scroll = gtk_scrolled_window_new(NULL, NULL);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll), GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_shadow_type(GTK_SCROLLED_WINDOW(scroll), GTK_SHADOW_IN);
    gtk_scrolled_window_set_min_content_height(GTK_SCROLLED_WINDOW(scroll), 120);
    GtkWidget* view = gtk_tree_view_new();
    gtk_container_add(GTK_CONTAINER(scroll), view);

    TableData* data = g_new0(TableData, 1);
    data->id = id;
    data->view = GTK_TREE_VIEW(view);
    data->selected = -1;
    g_object_set_data_full(G_OBJECT(scroll), TABLE_DATA_KEY, data, table_data_free);

    GtkTreeSelection* selection = gtk_tree_view_get_selection(data->view);
    gtk_tree_selection_set_mode(selection, GTK_SELECTION_SINGLE);
    g_signal_connect(selection, "changed", G_CALLBACK(on_selection_changed), data);
    g_signal_connect(view, "row-activated", G_CALLBACK(on_row_activated), data);
    gtk_widget_show_all(scroll);
    return (NGHandle)scroll;
}

int ng_linux_table_add_column(NGHandle table, const char* title) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (!title || data->store) return NG_ERROR_INVALID_PARAMETER;
    GtkCellRenderer* renderer = gtk_cell_renderer_text_new();
    g_object_set(renderer, "ellipsize", PANGO_ELLIPSIZE_END, NULL);
    GtkTreeViewColumn* column =
        gtk_tree_view_column_new_with_attributes(title, renderer, "text", data->columns, NULL);
    gtk_tree_view_column_set_resizable(column, TRUE);
    gtk_tree_view_column_set_expand(column, TRUE);
    gtk_tree_view_append_column(data->view, column);
    data->columns++;
    return NG_SUCCESS;
}

/* A store's columns are fixed, so it is made once the columns are known. */
static GtkListStore* store_of(TableData* data) {
    if (!data->store && data->columns > 0) {
        GType* types = g_new(GType, data->columns);
        for (int i = 0; i < data->columns; i++) types[i] = G_TYPE_STRING;
        data->store = gtk_list_store_newv(data->columns, types);
        g_free(types);
        gtk_tree_view_set_model(data->view, GTK_TREE_MODEL(data->store));
    }
    return data->store;
}

static int row_count(TableData* data) {
    return data->store ? gtk_tree_model_iter_n_children(GTK_TREE_MODEL(data->store), NULL) : 0;
}

static void set_cells(TableData* data, GtkTreeIter* iter, const char* const* cells, int count) {
    for (int i = 0; i < data->columns; i++) {
        gtk_list_store_set(data->store, iter, i, i < count && cells[i] ? cells[i] : "", -1);
    }
}

static int check_cells(TableData* data, const char* const* cells, int count) {
    if (count < 0 || count > data->columns || (count > 0 && !cells)) return NG_ERROR_INVALID_PARAMETER;
    return NG_SUCCESS;
}

/* Changes made by the app are not reported. */
static void quiet(TableData* data, int on) {
    ng_linux_set_quiet(gtk_tree_view_get_selection(data->view), on);
    if (!on) data->selected = view_selected(data);
}

int ng_linux_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index > row_count(data) || check_cells(data, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    if (!store_of(data)) return NG_ERROR_INVALID_PARAMETER;
    quiet(data, 1);
    GtkTreeIter iter;
    gtk_list_store_insert(data->store, &iter, index);
    set_cells(data, &iter, cells, count);
    quiet(data, 0);
    return NG_SUCCESS;
}

static int nth_row(TableData* data, int index, GtkTreeIter* iter) {
    return data->store && index >= 0 &&
           gtk_tree_model_iter_nth_child(GTK_TREE_MODEL(data->store), iter, NULL, index);
}

int ng_linux_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    GtkTreeIter iter;
    if (!nth_row(data, index, &iter) || check_cells(data, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    set_cells(data, &iter, cells, count);
    return NG_SUCCESS;
}

int ng_linux_table_remove_row(NGHandle table, int index) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    GtkTreeIter iter;
    if (!nth_row(data, index, &iter)) return NG_ERROR_INVALID_PARAMETER;
    quiet(data, 1);
    gtk_list_store_remove(data->store, &iter);
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_table_clear(NGHandle table) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (!data->store) return NG_SUCCESS;
    quiet(data, 1);
    gtk_list_store_clear(data->store);
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_table_set_selected(NGHandle table, int index) {
    TableData* data = table_data(table);
    if (!data) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= row_count(data)) return NG_ERROR_INVALID_PARAMETER;
    GtkTreeSelection* selection = gtk_tree_view_get_selection(data->view);
    quiet(data, 1);
    if (index < 0) {
        gtk_tree_selection_unselect_all(selection);
    } else {
        GtkTreePath* path = gtk_tree_path_new_from_indices(index, -1);
        gtk_tree_selection_select_path(selection, path);
        gtk_tree_view_scroll_to_cell(data->view, path, NULL, FALSE, 0, 0);
        gtk_tree_path_free(path);
    }
    quiet(data, 0);
    return NG_SUCCESS;
}

int ng_linux_table_get_selected(NGHandle table) {
    TableData* data = table_data(table);
    return data ? view_selected(data) : -1;
}
