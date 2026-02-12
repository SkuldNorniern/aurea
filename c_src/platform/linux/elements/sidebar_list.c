#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

extern void ng_invoke_sidebar_list_selected(unsigned int id, int index);

#define ROW_HEIGHT 18
#define INDENT_STEP 10

typedef enum { ROW_SECTION, ROW_ITEM } RowType;

typedef struct {
    unsigned int id;
    GtkListBox* listbox;
    GPtrArray* rows;
    int selected;
    int next_item_index;
} SidebarData;

typedef struct {
    RowType type;
    int item_index;
} RowInfo;

static void sidebar_data_free(gpointer p) {
    SidebarData* data = (SidebarData*)p;
    if (data->rows)
        g_ptr_array_free(data->rows, TRUE);
    g_free(data);
}

static void on_row_selected(GtkListBox* box, GtkListBoxRow* row, gpointer user_data) {
    SidebarData* data = (SidebarData*)user_data;
    if (!row) return;
    RowInfo* info = g_object_get_data(G_OBJECT(row), "row-info");
    if (!info || info->type != ROW_ITEM) return;
    if (info->item_index >= 0) {
        data->selected = info->item_index;
        ng_invoke_sidebar_list_selected(data->id, info->item_index);
    }
}

NGHandle ng_linux_create_sidebar_list(unsigned int id) {
    SidebarData* data = g_new0(SidebarData, 1);
    data->id = id;
    data->selected = -1;
    data->next_item_index = 0;
    data->rows = g_ptr_array_new_with_free_func(g_free);

    GtkWidget* listbox = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(listbox), GTK_SELECTION_SINGLE);
    g_signal_connect(listbox, "row-selected", G_CALLBACK(on_row_selected), data);
    g_object_set_data_full(G_OBJECT(listbox), "sidebar-data", data, sidebar_data_free);

    return (NGHandle)listbox;
}

int ng_linux_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    GtkListBox* box = GTK_LIST_BOX(sidebar);
    SidebarData* data = g_object_get_data(G_OBJECT(box), "sidebar-data");
    if (!data) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* row = gtk_list_box_row_new();
    gtk_list_box_row_set_selectable(GTK_LIST_BOX_ROW(row), FALSE);
    GtkWidget* label = gtk_label_new(title);
    gtk_widget_set_margin_start(label, 6);
    gtk_widget_set_margin_end(label, 6);
    gtk_widget_set_halign(label, GTK_ALIGN_START);
    PangoAttrList* attrs = pango_attr_list_new();
    pango_attr_list_insert(attrs, pango_attr_weight_new(PANGO_WEIGHT_BOLD));
    pango_attr_list_insert(attrs, pango_attr_scale_new(0.9));
    gtk_label_set_attributes(GTK_LABEL(label), attrs);
    pango_attr_list_unref(attrs);
    gtk_widget_set_margin_bottom(row, 4);
    gtk_container_add(GTK_CONTAINER(row), label);
    gtk_container_add(GTK_CONTAINER(box), row);

    RowInfo* info = g_new0(RowInfo, 1);
    info->type = ROW_SECTION;
    info->item_index = -1;
    g_object_set_data_full(G_OBJECT(row), "row-info", info, g_free);

    return NG_SUCCESS;
}

int ng_linux_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    GtkListBox* box = GTK_LIST_BOX(sidebar);
    SidebarData* data = g_object_get_data(G_OBJECT(box), "sidebar-data");
    if (!data) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* row = gtk_list_box_row_new();
    gtk_list_box_row_set_selectable(GTK_LIST_BOX_ROW(row), TRUE);
    GtkWidget* label = gtk_label_new(title);
    int margin = 6 + indent * INDENT_STEP;
    gtk_widget_set_margin_start(label, margin);
    gtk_widget_set_margin_end(label, 6);
    gtk_widget_set_halign(label, GTK_ALIGN_START);
    gtk_container_add(GTK_CONTAINER(row), label);
    gtk_container_add(GTK_CONTAINER(box), row);

    RowInfo* info = g_new0(RowInfo, 1);
    info->type = ROW_ITEM;
    info->item_index = data->next_item_index++;
    g_object_set_data_full(G_OBJECT(row), "row-info", info, g_free);

    return NG_SUCCESS;
}

static GtkListBoxRow* get_item_row(GtkListBox* box, int target_index) {
    GList* children = gtk_container_get_children(GTK_CONTAINER(box));
    GList* it = children;
    int idx = 0;
    GtkListBoxRow* found = NULL;
    while (it) {
        GtkWidget* row_w = GTK_WIDGET(it->data);
        RowInfo* info = g_object_get_data(G_OBJECT(row_w), "row-info");
        if (info && info->type == ROW_ITEM) {
            if (idx == target_index) {
                found = GTK_LIST_BOX_ROW(row_w);
                break;
            }
            idx++;
        }
        it = it->next;
    }
    g_list_free(children);
    return found;
}

int ng_linux_sidebar_list_set_selected(NGHandle sidebar, int index) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    GtkListBox* box = GTK_LIST_BOX(sidebar);
    SidebarData* data = g_object_get_data(G_OBJECT(box), "sidebar-data");
    if (!data) return NG_ERROR_INVALID_HANDLE;

    data->selected = index;
    GtkListBoxRow* row = get_item_row(box, index);
    if (row) {
        gtk_list_box_select_row(box, row);
    }
    return NG_SUCCESS;
}

int ng_linux_sidebar_list_get_selected(NGHandle sidebar) {
    if (!sidebar) return -1;
    GtkListBox* box = GTK_LIST_BOX(sidebar);
    SidebarData* data = g_object_get_data(G_OBJECT(box), "sidebar-data");
    return data ? data->selected : -1;
}

int ng_linux_sidebar_list_clear(NGHandle sidebar) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    GtkListBox* box = GTK_LIST_BOX(sidebar);
    SidebarData* data = g_object_get_data(G_OBJECT(box), "sidebar-data");
    if (!data) return NG_ERROR_INVALID_HANDLE;

    GList* children = gtk_container_get_children(GTK_CONTAINER(box));
    for (GList* it = children; it; it = it->next) {
        gtk_widget_destroy(GTK_WIDGET(it->data));
    }
    g_list_free(children);
    data->selected = -1;
    data->next_item_index = 0;
    return NG_SUCCESS;
}

void ng_linux_sidebar_list_invalidate(NGHandle sidebar) {
    if (!sidebar) return;
    gtk_widget_queue_draw((GtkWidget*)sidebar);
}
