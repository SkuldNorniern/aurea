#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

typedef struct {
    NgControl control;
    GPtrArray* buttons; /* in tab order, each held once */
    int selected;
    gboolean updating;
} TabBarState;

static const char tab_bar_kind = 0;

static void tab_bar_free(gpointer data) {
    TabBarState* state = data;
    g_ptr_array_free(state->buttons, TRUE);
    g_free(state);
}

static TabBarState* state_of(NGHandle tab_bar) {
    return GTK_IS_BOX(tab_bar) ? ng_control_typed(tab_bar, &tab_bar_kind) : NULL;
}

/* Setting a button's state can emit "clicked" on it, so reporting is off while the group is set. */
static void show_selected(TabBarState* state) {
    state->updating = TRUE;
    for (guint i = 0; i < state->buttons->len; i++) {
        GtkWidget* button = g_ptr_array_index(state->buttons, i);
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(button), (int)i == state->selected);
    }
    state->updating = FALSE;
}

static void on_tab_clicked(GtkButton* button, gpointer data) {
    TabBarState* state = data;
    guint index = 0;
    if (state->updating || !g_ptr_array_find(state->buttons, button, &index)) return;
    int tab = (int)index;
    int changed = tab != state->selected;
    state->selected = tab;
    show_selected(state);
    if (changed) ng_invoke_tab_bar_selected(state->control.id, tab);
}

NGHandle ng_linux_create_tab_bar(unsigned int id) {
    GtkWidget* box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);
    TabBarState* state = ng_control_attach(box, &tab_bar_kind, sizeof(TabBarState), tab_bar_free);
    state->control.id = id;
    state->selected = -1;
    state->buttons = g_ptr_array_new_with_free_func(g_object_unref);
    gtk_widget_show(box);
    return (NGHandle)box;
}

int ng_linux_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!title) return NG_ERROR_INVALID_PARAMETER;
    TabBarState* state = state_of(tab_bar);
    if (!state) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* button = gtk_toggle_button_new_with_label(title);
    g_signal_connect(button, "clicked", G_CALLBACK(on_tab_clicked), state);
    gtk_box_pack_start(GTK_BOX(tab_bar), button, FALSE, FALSE, 2);
    gtk_widget_show(button);
    g_ptr_array_add(state->buttons, g_object_ref_sink(button));
    if (state->selected < 0) state->selected = 0;
    show_selected(state);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    TabBarState* state = state_of(tab_bar);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)state->buttons->len) return NG_ERROR_INVALID_PARAMETER;

    gtk_container_remove(GTK_CONTAINER(tab_bar), g_ptr_array_index(state->buttons, index));
    g_ptr_array_remove_index(state->buttons, index);
    int count = (int)state->buttons->len;
    if (state->selected > index || state->selected >= count) state->selected--;
    if (state->selected < 0 && count > 0) state->selected = 0;
    show_selected(state);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_set_selected(NGHandle tab_bar, int index) {
    TabBarState* state = state_of(tab_bar);
    if (!state) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)state->buttons->len) return NG_ERROR_INVALID_PARAMETER;
    state->selected = index;
    show_selected(state);
    return NG_SUCCESS;
}

int ng_linux_tab_bar_get_selected(NGHandle tab_bar) {
    TabBarState* state = state_of(tab_bar);
    return state ? state->selected : -1;
}

void ng_linux_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    gtk_widget_queue_draw((GtkWidget*)tab_bar);
}
