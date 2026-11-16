#include "menu.h"
#include "window.h"
#include "common/shortcuts.h"
#include "common/menu_table.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <string.h>

/* An item carries its id under "menu-id"; "aurea-item" marks it as one (id 0 is NULL data). */
#define MENU_ID_KEY "menu-id"
#define MENU_ITEM_KEY "aurea-item"

static void menu_destroyed(GtkWidget* menu, gpointer data) {
    (void)data;
    ng_menu_table_forget_menu(menu);
}

static void register_menu_root(GtkWidget* menu, GtkWidget* root) {
    if (!menu) return;
    ng_menu_table_set_root(menu, root);
    g_signal_connect(menu, "destroy", G_CALLBACK(menu_destroyed), NULL);
}

static void menu_item_activated(GtkMenuItem* item, gpointer data) {
    (void)data;
    ng_invoke_menu_callback(GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(item), MENU_ID_KEY)));
}

static void attach_item(GtkWidget* item, unsigned int id) {
    g_object_set_data(G_OBJECT(item), MENU_ID_KEY, GUINT_TO_POINTER(id));
    g_object_set_data(G_OBJECT(item), MENU_ITEM_KEY, GINT_TO_POINTER(1));
    g_signal_connect(item, "activate", G_CALLBACK(menu_item_activated), NULL);
}

/* The item's id, or false when `child` is not an item. */
static gboolean item_id_of(GtkWidget* child, unsigned int* id) {
    if (!g_object_get_data(G_OBJECT(child), MENU_ITEM_KEY)) return FALSE;
    *id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(child), MENU_ID_KEY));
    return TRUE;
}

static GtkWidget* item_with_id(GtkWidget* menu, unsigned int id) {
    if (!GTK_IS_MENU_SHELL(menu)) return NULL;
    GtkWidget* found = NULL;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l && !found; l = l->next) {
        unsigned int child_id = 0;
        if (item_id_of(GTK_WIDGET(l->data), &child_id) && child_id == id) found = GTK_WIDGET(l->data);
    }
    g_list_free(children);
    return found;
}

/* The label GTK shows for a menu title: the text before a tab, with `&` marking a mnemonic. */
static char* menu_label(const char* title) {
    const char* tab = strchr(title, '\t');
    const char* end = tab ? tab : title + strlen(title);
    GString* out = g_string_sized_new((gsize)(end - title) + 8);
    for (const char* p = title; p < end; p++) {
        if (*p == '&') {
            if (p + 1 < end && p[1] == '&') {
                g_string_append_c(out, '&');
                p++;
            } else if (p + 1 < end) {
                g_string_append_c(out, '_');
            }
        } else if (*p == '_') {
            g_string_append(out, "__");
        } else {
            g_string_append_c(out, *p);
        }
    }
    return g_string_free(out, FALSE);
}

NGMenuHandle ng_linux_create_menu(void) {
    GtkWidget* menubar = gtk_menu_bar_new();
    g_object_ref_sink(menubar);
    register_menu_root(menubar, menubar);
    return (NGMenuHandle)menubar;
}

void ng_linux_destroy_menu(NGMenuHandle handle) {
    if (!handle || !GTK_IS_MENU_SHELL(handle)) return;
    gtk_widget_destroy((GtkWidget*)handle);
    g_object_unref(handle);
}

int ng_linux_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !GTK_IS_WINDOW(window) || !menu || !GTK_IS_MENU_SHELL(menu)) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* vbox = (GtkWidget*)ng_linux_window_get_content_view(window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;

    GtkWidget* menu_widget = (GtkWidget*)menu;
    GtkWidget* parent = gtk_widget_get_parent(menu_widget);
    if (parent && parent != vbox) {
        GtkWidget* old_window = gtk_widget_get_toplevel(parent);
        if (GTK_IS_WINDOW(old_window)) ng_menu_table_bind_window(old_window, NULL);
        g_object_ref(menu_widget);
        gtk_container_remove(GTK_CONTAINER(parent), menu_widget);
        gtk_box_pack_start(GTK_BOX(vbox), menu_widget, FALSE, FALSE, 0);
        g_object_unref(menu_widget);
    } else if (!parent) {
        gtk_box_pack_start(GTK_BOX(vbox), menu_widget, FALSE, FALSE, 0);
    }

    gtk_box_reorder_child(GTK_BOX(vbox), menu_widget, 0);
    ng_menu_table_bind_window(window, ng_menu_table_root(menu_widget));
    gtk_widget_show_all((GtkWidget*)window);
    return NG_SUCCESS;
}

NGMenuHandle ng_linux_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !GTK_IS_MENU_SHELL(parent_menu) || !title) return NULL;

    char* label = menu_label(title);
    GtkWidget* menu_item = gtk_menu_item_new_with_mnemonic(label);
    g_free(label);
    GtkWidget* submenu = gtk_menu_new();

    gtk_menu_item_set_submenu(GTK_MENU_ITEM(menu_item), submenu);
    gtk_menu_shell_append(GTK_MENU_SHELL(parent_menu), menu_item);
    gtk_widget_show_all(menu_item);

    register_menu_root(submenu, ng_menu_table_root(parent_menu));
    return (NGMenuHandle)submenu;
}

int ng_linux_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !GTK_IS_MENU_SHELL(menu) || !title) return NG_ERROR_INVALID_PARAMETER;

    char* label = menu_label(title);
    GtkWidget* menu_item = gtk_menu_item_new_with_mnemonic(label);
    g_free(label);
    attach_item(menu_item, id);
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), menu_item);
    gtk_widget_show(menu_item);
    ng_menu_table_add_shortcut(menu, title, id);
    return NG_SUCCESS;
}

int ng_linux_menu_move_last(NGMenuHandle menu, unsigned int index) {
    if (!menu || !GTK_IS_MENU(menu)) return NG_ERROR_INVALID_HANDLE;
    GList* rows = gtk_container_get_children(GTK_CONTAINER(menu));
    GList* last = g_list_last(rows);
    if (last) gtk_menu_reorder_child(GTK_MENU(menu), GTK_WIDGET(last->data), (gint)index);
    g_list_free(rows);
    return last ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}

int ng_linux_add_menu_separator(NGMenuHandle menu) {
    if (!menu || !GTK_IS_MENU_SHELL(menu)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* separator = gtk_separator_menu_item_new();
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), separator);
    gtk_widget_show(separator);
    return NG_SUCCESS;
}

int ng_linux_handle_menu_shortcut(NGHandle window, unsigned int keycode, unsigned int modifiers) {
    void* root = ng_menu_table_window_root(window);
    if (!root) return 0;

    void* menu;
    unsigned int id;
    for (int at = 0; ng_menu_table_next_shortcut(root, keycode, modifiers, &at, &menu, &id);) {
        GtkWidget* item = item_with_id((GtkWidget*)menu, id);
        if (item && !gtk_widget_is_sensitive(item)) continue;
        ng_invoke_menu_callback(id);
        return 1;
    }
    return 0;
}

int ng_linux_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio) {
    if (!menu || !GTK_IS_MENU_SHELL(menu) || !title) return NG_ERROR_INVALID_PARAMETER;
    char* label = menu_label(title);
    GtkWidget* item = gtk_check_menu_item_new_with_mnemonic(label);
    g_free(label);
    gtk_check_menu_item_set_draw_as_radio(GTK_CHECK_MENU_ITEM(item), radio ? TRUE : FALSE);
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    attach_item(item, id);
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), item);
    gtk_widget_show(item);
    ng_menu_table_add_shortcut(menu, title, id);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_set_sensitive(item, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

/* Setting a check does not emit activate, so it reports nothing. */
int ng_linux_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !GTK_IS_CHECK_MENU_ITEM(item)) return NG_ERROR_INVALID_PARAMETER;
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !title) return NG_ERROR_INVALID_PARAMETER;
    char* label = menu_label(title);
    gtk_menu_item_set_use_underline(GTK_MENU_ITEM(item), TRUE);
    gtk_menu_item_set_label(GTK_MENU_ITEM(item), label);
    g_free(label);
    ng_menu_table_forget_item(id);
    ng_menu_table_add_shortcut(menu, title, id);
    return NG_SUCCESS;
}

int ng_linux_menu_item_remove(NGMenuHandle menu, unsigned int id) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_destroy(item);
    ng_menu_table_forget_item(id);
    return NG_SUCCESS;
}

int ng_linux_menu_clear(NGMenuHandle menu) {
    if (!menu || !GTK_IS_MENU_SHELL(menu)) return NG_ERROR_INVALID_HANDLE;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l; l = l->next) {
        GtkWidget* child = GTK_WIDGET(l->data);
        unsigned int id = 0;
        if (item_id_of(child, &id)) ng_menu_table_forget_item(id);
        gtk_widget_destroy(child);
    }
    g_list_free(children);
    return NG_SUCCESS;
}
