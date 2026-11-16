#include "menu.h"
#include "window.h"
#include "common/shortcuts.h"
#include "common/menu_table.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

static void menu_destroyed(GtkWidget* menu, gpointer data) {
    (void)data;
    ng_menu_table_forget_menu(menu);
}

static void register_menu_root(GtkWidget* menu, GtkWidget* root) {
    if (!menu) return;
    ng_menu_table_set_root(menu, root);
    g_signal_connect(menu, "destroy", G_CALLBACK(menu_destroyed), NULL);
}

static void menu_item_clicked(GtkMenuItem *item, gpointer user_data) {
    (void)user_data;
    guint id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(item), "menu-id"));
    ng_invoke_menu_callback(id);
}

static GtkWidget* item_with_id(GtkWidget* menu, unsigned int id) {
    if (!menu) return NULL;
    GtkWidget* found = NULL;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l && !found; l = l->next) {
        GObject* child = G_OBJECT(l->data);
        if (g_object_get_data(child, "aurea-item") &&
            GPOINTER_TO_UINT(g_object_get_data(child, "menu-id")) == id) {
            found = GTK_WIDGET(child);
        }
    }
    g_list_free(children);
    return found;
}

NGMenuHandle ng_linux_create_menu(void) {
    GtkWidget *menubar = gtk_menu_bar_new();
    g_object_ref_sink(menubar);
    register_menu_root(menubar, menubar);
    return (NGMenuHandle)menubar;
}

void ng_linux_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
    g_object_unref(handle);
}

int ng_linux_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* vbox = (GtkWidget*)ng_linux_window_get_content_view(window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;

    GtkWidget* menu_widget = (GtkWidget*)menu;
    GtkWidget* parent = gtk_widget_get_parent(menu_widget);
    if (parent && parent != vbox) {
        gtk_container_remove(GTK_CONTAINER(parent), menu_widget);
    }
    if (!gtk_widget_get_parent(menu_widget)) {
        gtk_box_pack_start(GTK_BOX(vbox), menu_widget, FALSE, FALSE, 0);
    }

    gtk_box_reorder_child(GTK_BOX(vbox), menu_widget, 0);
    ng_menu_table_bind_window(window, ng_menu_table_root(menu_widget));
    gtk_widget_show_all((GtkWidget*)window);
    return NG_SUCCESS;
}

static const char* display_title(const char* title, char* buf, size_t buf_size) {
    const char* tab = strchr(title, '\t');
    if (!tab || (size_t)(tab - title) >= buf_size) return title;
    size_t len = (size_t)(tab - title);
    memcpy(buf, title, len);
    buf[len] = '\0';
    return buf;
}

static const char* gtk_mnemonic_label(const char* title, char* buf, size_t buf_size) {
    size_t out = 0;
    for (const char* p = title; *p && out + 2 < buf_size; p++) {
        if (*p == '&') {
            if (p[1] == '&') {
                buf[out++] = '&';
                p++;
            } else if (p[1]) {
                buf[out++] = '_';
            }
        } else if (*p == '_') {
            buf[out++] = '_';
            buf[out++] = '_';
        } else {
            buf[out++] = *p;
        }
    }
    buf[out] = '\0';
    return buf;
}

NGMenuHandle ng_linux_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));
    char label_buf[512];
    GtkWidget *menu_item = gtk_menu_item_new_with_mnemonic(gtk_mnemonic_label(label, label_buf, sizeof(label_buf)));
    GtkWidget *submenu = gtk_menu_new();
    
    gtk_menu_item_set_submenu(GTK_MENU_ITEM(menu_item), submenu);
    gtk_menu_shell_append(GTK_MENU_SHELL(parent_menu), menu_item);
    gtk_widget_show_all(menu_item);

    register_menu_root(submenu, ng_menu_table_root(parent_menu));
    
    return (NGMenuHandle)submenu;
}

int ng_linux_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;

    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));
    char label_buf[512];

    GtkWidget *menu_item = gtk_menu_item_new_with_mnemonic(gtk_mnemonic_label(label, label_buf, sizeof(label_buf)));
    g_object_set_data(G_OBJECT(menu_item), "menu-id", GUINT_TO_POINTER(id));
    g_object_set_data(G_OBJECT(menu_item), "aurea-item", GINT_TO_POINTER(1));
    g_signal_connect(G_OBJECT(menu_item), "activate", 
                     G_CALLBACK(menu_item_clicked), NULL);
    
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
    if (!menu) return NG_ERROR_INVALID_HANDLE;
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
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    char display_buf[256];
    char label_buf[512];
    GtkWidget* item = gtk_check_menu_item_new_with_mnemonic(
        gtk_mnemonic_label(display_title(title, display_buf, sizeof(display_buf)), label_buf, sizeof(label_buf)));
    gtk_check_menu_item_set_draw_as_radio(GTK_CHECK_MENU_ITEM(item), radio ? TRUE : FALSE);
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    g_object_set_data(G_OBJECT(item), "menu-id", GUINT_TO_POINTER(id));
    g_object_set_data(G_OBJECT(item), "aurea-item", GINT_TO_POINTER(1));
    g_signal_connect(G_OBJECT(item), "activate", G_CALLBACK(menu_item_clicked), NULL);
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

int ng_linux_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !GTK_IS_CHECK_MENU_ITEM(item)) return NG_ERROR_INVALID_PARAMETER;
    g_signal_handlers_block_by_func(item, G_CALLBACK(menu_item_clicked), NULL);
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    g_signal_handlers_unblock_by_func(item, G_CALLBACK(menu_item_clicked), NULL);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !title) return NG_ERROR_INVALID_PARAMETER;
    char display_buf[256];
    char label_buf[512];
    gtk_menu_item_set_use_underline(GTK_MENU_ITEM(item), TRUE);
    gtk_menu_item_set_label(GTK_MENU_ITEM(item),
        gtk_mnemonic_label(display_title(title, display_buf, sizeof(display_buf)), label_buf, sizeof(label_buf)));
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
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l; l = l->next) {
        GObject* child = G_OBJECT(l->data);
        if (g_object_get_data(child, "aurea-item")) {
            ng_menu_table_forget_item(GPOINTER_TO_UINT(g_object_get_data(child, "menu-id")));
        }
        gtk_widget_destroy(GTK_WIDGET(child));
    }
    g_list_free(children);
    return NG_SUCCESS;
}
