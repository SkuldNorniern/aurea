#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

typedef struct {
    NgControl control;
    GtkListBox* box;
    int selected;
} RowListState;

static const char row_list_kind = 0;

static RowListState* state_of(NGHandle list) {
    return GTK_IS_SCROLLED_WINDOW(list) ? ng_control_typed(list, &row_list_kind) : NULL;
}

static int selected_index(RowListState* state) {
    GtkListBoxRow* row = gtk_list_box_get_selected_row(state->box);
    return row ? gtk_list_box_row_get_index(row) : -1;
}

static void on_row_selected(GtkListBox* box, GtkListBoxRow* row, gpointer data) {
    (void)box;
    (void)row;
    RowListState* state = data;
    if (state->control.quiet) return;
    int index = selected_index(state);
    if (index != state->selected) {
        state->selected = index;
        ng_invoke_row_list_selected(state->control.id, index);
    }
}

static void on_row_activated(GtkListBox* box, GtkListBoxRow* row, gpointer data) {
    (void)box;
    RowListState* state = data;
    ng_invoke_row_list_activated(state->control.id, gtk_list_box_row_get_index(row));
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

    RowListState* state = ng_control_attach(scroll, &row_list_kind, sizeof(RowListState), g_free);
    state->control.id = id;
    state->box = GTK_LIST_BOX(box);
    state->selected = -1;
    g_signal_connect(box, "row-selected", G_CALLBACK(on_row_selected), state);
    g_signal_connect(box, "row-activated", G_CALLBACK(on_row_activated), state);
    gtk_widget_show_all(scroll);
    return (NGHandle)scroll;
}

static int row_count(RowListState* state) {
    GList* rows = gtk_container_get_children(GTK_CONTAINER(state->box));
    int count = (int)g_list_length(rows);
    g_list_free(rows);
    return count;
}

/* Changes made by the app are not reported. */
static void set_silent(RowListState* state, int on) {
    state->control.quiet = on ? TRUE : FALSE;
    if (!on) state->selected = selected_index(state);
}

int ng_linux_row_list_insert(NGHandle list, int index, NGHandle element) {
    RowListState* state = state_of(list);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (!element || !GTK_IS_WIDGET(element) || index < 0 || index > row_count(state)) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    GtkWidget* widget = (GtkWidget*)element;
    g_object_ref(widget);
    ng_linux_unparent(widget);
    set_silent(state, 1);
    gtk_list_box_insert(state->box, widget, index);
    set_silent(state, 0);
    g_object_unref(widget);
    gtk_widget_show(widget);
    gtk_widget_show(gtk_widget_get_parent(widget));
    return NG_SUCCESS;
}

int ng_linux_row_list_remove(NGHandle list, int index) {
    RowListState* state = state_of(list);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    GtkListBoxRow* row = gtk_list_box_get_row_at_index(state->box, index);
    if (!row) return NG_ERROR_INVALID_PARAMETER;
    set_silent(state, 1);
    gtk_widget_destroy(GTK_WIDGET(row));
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_clear(NGHandle list) {
    RowListState* state = state_of(list);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    set_silent(state, 1);
    GList* rows = gtk_container_get_children(GTK_CONTAINER(state->box));
    for (GList* at = rows; at; at = at->next) gtk_widget_destroy(GTK_WIDGET(at->data));
    g_list_free(rows);
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_set_selected(NGHandle list, int index) {
    RowListState* state = state_of(list);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= row_count(state)) return NG_ERROR_INVALID_PARAMETER;
    set_silent(state, 1);
    if (index < 0) {
        gtk_list_box_unselect_all(state->box);
    } else {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(state->box, index);
        gtk_list_box_select_row(state->box, row);
        GtkAdjustment* v = gtk_scrolled_window_get_vadjustment(GTK_SCROLLED_WINDOW(list));
        GtkAllocation at;
        gtk_widget_get_allocation(GTK_WIDGET(row), &at);
        if (at.height > 0) gtk_adjustment_clamp_page(v, at.y, at.y + at.height);
    }
    set_silent(state, 0);
    return NG_SUCCESS;
}

int ng_linux_row_list_get_selected(NGHandle list) {
    RowListState* state = state_of(list);
    return state ? selected_index(state) : -1;
}
