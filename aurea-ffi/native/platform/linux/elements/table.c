#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

typedef struct {
    NgControl control;
    GtkTreeView* view;
    GtkListStore* store;
    int columns;
    int selected;
} TableState;

static const char table_kind = 0;

static void table_free(gpointer data) {
    TableState* state = data;
    g_clear_object(&state->store);
    g_free(state);
}

static TableState* state_of(NGHandle table) {
    return GTK_IS_SCROLLED_WINDOW(table) ? ng_control_typed(table, &table_kind) : NULL;
}

static int view_selected(TableState* state) {
    GtkTreeModel* model = NULL;
    GtkTreeIter iter;
    if (!gtk_tree_selection_get_selected(gtk_tree_view_get_selection(state->view), &model, &iter)) return -1;
    GtkTreePath* path = gtk_tree_model_get_path(model, &iter);
    int row = gtk_tree_path_get_indices(path)[0];
    gtk_tree_path_free(path);
    return row;
}

static void on_selection_changed(GtkTreeSelection* selection, gpointer data) {
    (void)selection;
    TableState* state = data;
    if (state->control.quiet) return;
    int row = view_selected(state);
    if (row != state->selected) {
        state->selected = row;
        ng_invoke_table_selected(state->control.id, row);
    }
}

static void on_row_activated(GtkTreeView* view, GtkTreePath* path, GtkTreeViewColumn* column, gpointer data) {
    (void)view;
    (void)column;
    TableState* state = data;
    ng_invoke_table_activated(state->control.id, gtk_tree_path_get_indices(path)[0]);
}

NGHandle ng_linux_create_table(unsigned int id) {
    GtkWidget* scroll = gtk_scrolled_window_new(NULL, NULL);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll), GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_shadow_type(GTK_SCROLLED_WINDOW(scroll), GTK_SHADOW_IN);
    gtk_scrolled_window_set_min_content_height(GTK_SCROLLED_WINDOW(scroll), 120);
    GtkWidget* view = gtk_tree_view_new();
    gtk_container_add(GTK_CONTAINER(scroll), view);

    TableState* state = ng_control_attach(scroll, &table_kind, sizeof(TableState), table_free);
    state->control.id = id;
    state->view = GTK_TREE_VIEW(view);
    state->selected = -1;

    GtkTreeSelection* selection = gtk_tree_view_get_selection(state->view);
    gtk_tree_selection_set_mode(selection, GTK_SELECTION_SINGLE);
    g_signal_connect(selection, "changed", G_CALLBACK(on_selection_changed), state);
    g_signal_connect(view, "row-activated", G_CALLBACK(on_row_activated), state);
    gtk_widget_show_all(scroll);
    return (NGHandle)scroll;
}

int ng_linux_table_add_column(NGHandle table, const char* title) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (!title || state->store) return NG_ERROR_INVALID_PARAMETER;
    GtkCellRenderer* renderer = gtk_cell_renderer_text_new();
    g_object_set(renderer, "ellipsize", PANGO_ELLIPSIZE_END, NULL);
    GtkTreeViewColumn* column = gtk_tree_view_column_new_with_attributes(title, renderer, "text", state->columns, NULL);
    gtk_tree_view_column_set_resizable(column, TRUE);
    gtk_tree_view_column_set_expand(column, TRUE);
    gtk_tree_view_append_column(state->view, column);
    state->columns++;
    return NG_SUCCESS;
}

/* A store's columns are fixed, so it is made once the columns are known. */
static GtkListStore* store_of(TableState* state) {
    if (!state->store && state->columns > 0) {
        GType* types = g_new(GType, state->columns);
        for (int i = 0; i < state->columns; i++) types[i] = G_TYPE_STRING;
        state->store = gtk_list_store_newv(state->columns, types);
        g_free(types);
        gtk_tree_view_set_model(state->view, GTK_TREE_MODEL(state->store));
    }
    return state->store;
}

static int row_count(TableState* state) {
    return state->store ? gtk_tree_model_iter_n_children(GTK_TREE_MODEL(state->store), NULL) : 0;
}

static void set_cells(TableState* state, GtkTreeIter* iter, const char* const* cells, int count) {
    for (int i = 0; i < state->columns; i++) {
        gtk_list_store_set(state->store, iter, i, i < count && cells[i] ? cells[i] : "", -1);
    }
}

static int check_cells(TableState* state, const char* const* cells, int count) {
    if (count < 0 || count > state->columns || (count > 0 && !cells)) return NG_ERROR_INVALID_PARAMETER;
    return NG_SUCCESS;
}

/* Changes made by the app are not reported. */
static void set_silent(TableState* state, int on) {
    state->control.quiet = on ? TRUE : FALSE;
    if (!on) state->selected = view_selected(state);
}

int ng_linux_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index > row_count(state) || check_cells(state, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    if (!store_of(state)) return NG_ERROR_INVALID_PARAMETER;
    set_silent(state, 1);
    GtkTreeIter iter;
    gtk_list_store_insert(state->store, &iter, index);
    set_cells(state, &iter, cells, count);
    set_silent(state, 0);
    return NG_SUCCESS;
}

static int nth_row(TableState* state, int index, GtkTreeIter* iter) {
    return state->store && index >= 0 &&
           gtk_tree_model_iter_nth_child(GTK_TREE_MODEL(state->store), iter, NULL, index);
}

int ng_linux_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    GtkTreeIter iter;
    if (!nth_row(state, index, &iter) || check_cells(state, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    set_cells(state, &iter, cells, count);
    return NG_SUCCESS;
}

int ng_linux_table_remove_row(NGHandle table, int index) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    GtkTreeIter iter;
    if (!nth_row(state, index, &iter)) return NG_ERROR_INVALID_PARAMETER;
    set_silent(state, 1);
    gtk_list_store_remove(state->store, &iter);
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_table_clear(NGHandle table) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (!state->store) return NG_SUCCESS;
    set_silent(state, 1);
    gtk_list_store_clear(state->store);
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_table_set_selected(NGHandle table, int index) {
    TableState* state = state_of(table);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= row_count(state)) return NG_ERROR_INVALID_PARAMETER;
    GtkTreeSelection* selection = gtk_tree_view_get_selection(state->view);
    set_silent(state, 1);
    if (index < 0) {
        gtk_tree_selection_unselect_all(selection);
    } else {
        GtkTreePath* path = gtk_tree_path_new_from_indices(index, -1);
        gtk_tree_selection_select_path(selection, path);
        gtk_tree_view_scroll_to_cell(state->view, path, NULL, FALSE, 0, 0);
        gtk_tree_path_free(path);
    }
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_table_get_selected(NGHandle table) {
    TableState* state = state_of(table);
    return state ? view_selected(state) : -1;
}
