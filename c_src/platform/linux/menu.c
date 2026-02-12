#include "menu.h"
#include "window.h"
#include "common/errors.h"
#include <gtk/gtk.h>

extern void ng_invoke_menu_callback(unsigned int id);

static void menu_item_clicked(GtkMenuItem *item, gpointer user_data) {
    guint id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(item), "menu-id"));
    ng_invoke_menu_callback(id);
}

NGMenuHandle ng_linux_create_menu(void) {
    GtkWidget *menubar = gtk_menu_bar_new();
    GtkWidget* vbox = (GtkWidget*)ng_linux_get_main_vbox();
    
    if (vbox) {
        gtk_box_pack_start(GTK_BOX(vbox), menubar, FALSE, FALSE, 0);
        gtk_widget_show(menubar);
    }
    
    return (NGMenuHandle)menubar;
}

void ng_linux_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

int ng_linux_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_show_all((GtkWidget*)window);
    return NG_SUCCESS;
}

NGMenuHandle ng_linux_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    GtkWidget *menu_item = gtk_menu_item_new_with_label(title);
    GtkWidget *submenu = gtk_menu_new();
    
    gtk_menu_item_set_submenu(GTK_MENU_ITEM(menu_item), submenu);
    gtk_menu_shell_append(GTK_MENU_SHELL(parent_menu), menu_item);
    gtk_widget_show_all(menu_item);
    
    return (NGMenuHandle)submenu;
}

int ng_linux_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget *menu_item = gtk_menu_item_new_with_label(title);
    g_object_set_data(G_OBJECT(menu_item), "menu-id", GUINT_TO_POINTER(id));
    g_signal_connect(G_OBJECT(menu_item), "activate", 
                     G_CALLBACK(menu_item_clicked), NULL);
    
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), menu_item);
    gtk_widget_show(menu_item);
    
    return NG_SUCCESS;
}

int ng_linux_add_menu_separator(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* separator = gtk_separator_menu_item_new();
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), separator);
    gtk_widget_show(separator);
    return NG_SUCCESS;
}
