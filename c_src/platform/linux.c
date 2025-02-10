#include "linux.h"
#include "../common/errors.h"
#include <gtk/gtk.h>

static gboolean gtk_initialized = FALSE;

int ng_platform_init(void) {
    if (!gtk_initialized) {
        int argc = 0;
        char **argv = NULL;
        gtk_init(&argc, &argv);
        gtk_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_platform_cleanup(void) {
    if (gtk_initialized) {
        gtk_initialized = FALSE;
    }
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), title);
    gtk_window_set_default_size(GTK_WINDOW(window), width, height);
    gtk_widget_show_all(window);
    
    return (NGHandle)window;
}

void ng_platform_destroy_window(NGHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

NGMenuHandle ng_platform_create_menu(void) {
    GtkWidget *menubar = gtk_menu_bar_new();
    gtk_widget_show(menubar);
    return (NGMenuHandle)menubar;
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    
    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), vbox);
    gtk_box_pack_start(GTK_BOX(vbox), (GtkWidget*)menu, FALSE, FALSE, 0);
    gtk_widget_show_all((GtkWidget*)window);
    
    return NG_SUCCESS;
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget *menu_item = gtk_menu_item_new_with_label(title);
    g_object_set_data(G_OBJECT(menu_item), "menu-id", GUINT_TO_POINTER(id));
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), menu_item);
    gtk_widget_show(menu_item);
    
    return NG_SUCCESS;
}

int ng_platform_run(void) {
    gtk_main();
    return NG_SUCCESS;
}

// ... rest of Linux/GTK implementation ... 