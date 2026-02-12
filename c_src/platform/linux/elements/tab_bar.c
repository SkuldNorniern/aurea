#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

extern void ng_invoke_tab_bar_selected(unsigned int id, int index);

typedef struct {
    unsigned int id;
    GPtrArray* buttons;
    int selected;
} TabBarData;

static void tabbar_data_free(gpointer p) {
    TabBarData* data = (TabBarData*)p;
    if (data->buttons)
        g_ptr_array_free(data->buttons, TRUE);
    g_free(data);
}

static void on_tab_clicked(GtkButton* btn, gpointer user_data) {
    TabBarData* data = (TabBarData*)user_data;
    int idx = GPOINTER_TO_INT(g_object_get_data(G_OBJECT(btn), "tab-index"));
    if (idx >= 0 && idx != data->selected) {
        if (data->selected >= 0 && data->selected < (int)data->buttons->len) {
            GtkWidget* prev = g_ptr_array_index(data->buttons, data->selected);
            gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(prev), FALSE);
        }
        data->selected = idx;
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(btn), TRUE);
        ng_invoke_tab_bar_selected(data->id, idx);
    }
}

NGHandle ng_linux_create_tab_bar(unsigned int id) {
    TabBarData* data = g_new0(TabBarData, 1);
    data->id = id;
    data->selected = 0;
    data->buttons = g_ptr_array_new_with_free_func((GDestroyNotify)g_object_unref);
    GtkWidget* box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);
    g_object_set_data_full(G_OBJECT(box), "tabbar-data", data, tabbar_data_free);
    return (NGHandle)box;
}

int ng_linux_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return -1;
    GtkWidget* box = (GtkWidget*)tab_bar;
    TabBarData* data = g_object_get_data(G_OBJECT(box), "tabbar-data");
    if (!data) return -1;
    GtkWidget* btn = gtk_toggle_button_new_with_label(title);
    int idx = data->buttons->len;
    g_object_set_data(G_OBJECT(btn), "tab-index", GINT_TO_POINTER(idx));
    g_signal_connect(btn, "clicked", G_CALLBACK(on_tab_clicked), data);
    gtk_box_pack_start(GTK_BOX(box), btn, FALSE, FALSE, 2);
    g_ptr_array_add(data->buttons, g_object_ref_sink(btn));
    if (idx == 0) {
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(btn), TRUE);
        data->selected = 0;
    }
    return 0;
}

int ng_linux_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    (void)tab_bar;
    (void)index;
    return 0;
}

int ng_linux_tab_bar_set_selected(NGHandle tab_bar, int index) {
    if (!tab_bar) return -1;
    GtkWidget* box = (GtkWidget*)tab_bar;
    TabBarData* data = g_object_get_data(G_OBJECT(box), "tabbar-data");
    if (!data || index < 0 || index >= (int)data->buttons->len) return -1;
    if (data->selected >= 0 && data->selected < (int)data->buttons->len) {
        GtkWidget* prev = g_ptr_array_index(data->buttons, data->selected);
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(prev), FALSE);
    }
    data->selected = index;
    GtkWidget* btn = g_ptr_array_index(data->buttons, index);
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(btn), TRUE);
    return 0;
}

int ng_linux_tab_bar_get_selected(NGHandle tab_bar) {
    if (!tab_bar) return -1;
    GtkWidget* box = (GtkWidget*)tab_bar;
    TabBarData* data = g_object_get_data(G_OBJECT(box), "tabbar-data");
    return data ? data->selected : -1;
}

void ng_linux_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    gtk_widget_queue_draw((GtkWidget*)tab_bar);
}
