#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

#define INDENT_STEP 10

typedef struct {
    NgControl control;
    int selected;
    int next_item_index;
} SidebarState;

/* A row's state: whether it is a section or an item, and which item it is. */
typedef struct {
    NgControl control;
    gboolean item;
    int item_index;
} RowState;

static const char sidebar_kind = 0;
static const char row_kind = 0;

static SidebarState* sidebar_of(NGHandle sidebar) {
    return GTK_IS_LIST_BOX(sidebar) ? ng_control_typed(sidebar, &sidebar_kind) : NULL;
}

static RowState* row_of(GtkWidget* row) {
    return GTK_IS_LIST_BOX_ROW(row) ? ng_control_typed(row, &row_kind) : NULL;
}

static void on_row_selected(GtkListBox* box, GtkListBoxRow* row, gpointer data) {
    (void)box;
    SidebarState* state = data;
    if (!row || state->control.quiet) return;
    RowState* info = row_of(GTK_WIDGET(row));
    if (!info || !info->item) return;
    state->selected = info->item_index;
    ng_invoke_sidebar_list_selected(state->control.id, info->item_index);
}

static void sidebar_row_attach(GtkWidget* row, gboolean item, int item_index) {
    RowState* info = ng_control_attach(row, &row_kind, sizeof(RowState), g_free);
    info->item = item;
    info->item_index = item_index;
}

static void label_attrs_section(GtkWidget* label) {
    PangoAttrList* attrs = pango_attr_list_new();
    pango_attr_list_insert(attrs, pango_attr_weight_new(PANGO_WEIGHT_BOLD));
    pango_attr_list_insert(attrs, pango_attr_scale_new(0.9));
    gtk_label_set_attributes(GTK_LABEL(label), attrs);
    pango_attr_list_unref(attrs);
}

NGHandle ng_linux_create_sidebar_list(unsigned int id) {
    GtkWidget* listbox = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(listbox), GTK_SELECTION_SINGLE);
    SidebarState* state = ng_control_attach(listbox, &sidebar_kind, sizeof(SidebarState), g_free);
    state->control.id = id;
    state->selected = -1;
    g_signal_connect(listbox, "row-selected", G_CALLBACK(on_row_selected), state);
    return (NGHandle)listbox;
}

int ng_linux_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    SidebarState* state = sidebar_of(sidebar);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (!title) return NG_ERROR_INVALID_PARAMETER;

    GtkWidget* row = gtk_list_box_row_new();
    gtk_list_box_row_set_selectable(GTK_LIST_BOX_ROW(row), FALSE);
    GtkWidget* label = gtk_label_new(title);
    gtk_widget_set_margin_start(label, 6);
    gtk_widget_set_margin_end(label, 6);
    gtk_widget_set_halign(label, GTK_ALIGN_START);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    label_attrs_section(label);
    gtk_widget_set_margin_bottom(row, 4);
    gtk_container_add(GTK_CONTAINER(row), label);
    gtk_container_add(GTK_CONTAINER(sidebar), row);
    gtk_widget_show_all(row);
    sidebar_row_attach(row, FALSE, -1);
    return NG_SUCCESS;
}

int ng_linux_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    SidebarState* state = sidebar_of(sidebar);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (!title) return NG_ERROR_INVALID_PARAMETER;

    GtkWidget* row = gtk_list_box_row_new();
    gtk_list_box_row_set_selectable(GTK_LIST_BOX_ROW(row), TRUE);
    GtkWidget* label = gtk_label_new(title);
    int steps = indent > 0 ? (indent > 0x7FFF ? 0x7FFF : indent) : 0;
    gtk_widget_set_margin_start(label, 6 + steps * INDENT_STEP);
    gtk_widget_set_margin_end(label, 6);
    gtk_widget_set_halign(label, GTK_ALIGN_START);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_container_add(GTK_CONTAINER(row), label);
    gtk_container_add(GTK_CONTAINER(sidebar), row);
    gtk_widget_show_all(row);
    sidebar_row_attach(row, TRUE, state->next_item_index++);
    return NG_SUCCESS;
}

static GtkListBoxRow* get_item_row(GtkListBox* box, int target_index) {
    GtkListBoxRow* found = NULL;
    GList* children = gtk_container_get_children(GTK_CONTAINER(box));
    for (GList* it = children; it && !found; it = it->next) {
        RowState* info = row_of(GTK_WIDGET(it->data));
        if (info && info->item && info->item_index == target_index) found = GTK_LIST_BOX_ROW(it->data);
    }
    g_list_free(children);
    return found;
}

int ng_linux_sidebar_list_set_selected(NGHandle sidebar, int index) {
    SidebarState* state = sidebar_of(sidebar);
    if (!state) return NG_ERROR_INVALID_HANDLE;

    GtkListBoxRow* row = index >= 0 ? get_item_row(GTK_LIST_BOX(sidebar), index) : NULL;
    if (index != -1 && !row) return NG_ERROR_INVALID_PARAMETER;
    state->selected = index;
    state->control.quiet = TRUE;
    if (row) {
        gtk_list_box_select_row(GTK_LIST_BOX(sidebar), row);
    } else {
        gtk_list_box_unselect_all(GTK_LIST_BOX(sidebar));
    }
    state->control.quiet = FALSE;
    return NG_SUCCESS;
}

int ng_linux_sidebar_list_get_selected(NGHandle sidebar) {
    SidebarState* state = sidebar_of(sidebar);
    return state ? state->selected : -1;
}

int ng_linux_sidebar_list_clear(NGHandle sidebar) {
    SidebarState* state = sidebar_of(sidebar);
    if (!state) return NG_ERROR_INVALID_HANDLE;

    GList* children = gtk_container_get_children(GTK_CONTAINER(sidebar));
    for (GList* it = children; it; it = it->next) gtk_widget_destroy(GTK_WIDGET(it->data));
    g_list_free(children);
    state->selected = -1;
    state->next_item_index = 0;
    return NG_SUCCESS;
}

void ng_linux_sidebar_list_invalidate(NGHandle sidebar) {
    if (!sidebar) return;
    gtk_widget_queue_draw((GtkWidget*)sidebar);
}
