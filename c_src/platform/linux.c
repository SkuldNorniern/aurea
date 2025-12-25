#include "linux.h"
#include "../common/errors.h"
#include <gtk/gtk.h>

static gboolean gtk_initialized = FALSE;
static GtkWidget *main_vbox = NULL;

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

static void on_window_destroy(GtkWidget* widget, gpointer data) {
    // Quit the GTK main loop when window is closed
    gtk_main_quit();
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), title);
    gtk_window_set_default_size(GTK_WINDOW(window), width, height);
    
    // Connect destroy signal to quit the event loop
    g_signal_connect(G_OBJECT(window), "destroy", G_CALLBACK(on_window_destroy), NULL);
    
    // Create a vertical box to hold menu and content
    main_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), main_vbox);
    
    gtk_widget_show_all(window);
    
    return (NGHandle)window;
}

void ng_platform_destroy_window(NGHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

NGMenuHandle ng_platform_create_menu(void) {
    GtkWidget *menubar = gtk_menu_bar_new();
    gtk_box_pack_start(GTK_BOX(main_vbox), menubar, FALSE, FALSE, 0);
    gtk_widget_show(menubar);
    return (NGMenuHandle)menubar;
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_show_all((GtkWidget*)window);
    return NG_SUCCESS;
}

static void menu_item_clicked(GtkMenuItem *item, gpointer user_data) {
    guint id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(item), "menu-id"));
    g_print("Menu item clicked: %u\n", id);
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    GtkWidget *menu_item = gtk_menu_item_new_with_label(title);
    GtkWidget *submenu = gtk_menu_new();
    
    gtk_menu_item_set_submenu(GTK_MENU_ITEM(menu_item), submenu);
    gtk_menu_shell_append(GTK_MENU_SHELL(parent_menu), menu_item);
    gtk_widget_show_all(menu_item);
    
    return (NGMenuHandle)submenu;
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget *menu_item = gtk_menu_item_new_with_label(title);
    g_object_set_data(G_OBJECT(menu_item), "menu-id", GUINT_TO_POINTER(id));
    g_signal_connect(G_OBJECT(menu_item), "activate", 
                     G_CALLBACK(menu_item_clicked), NULL);
    
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), menu_item);
    gtk_widget_show(menu_item);
    
    return NG_SUCCESS;
}

int ng_platform_run(void) {
    gtk_main();
    return NG_SUCCESS;
}

NGHandle ng_platform_create_canvas(int width, int height) {
    // Create a GtkDrawingArea for custom rendering
    // This will be extended to support OpenGL/Vulkan
    GtkWidget* drawing_area = gtk_drawing_area_new();
    gtk_widget_set_size_request(drawing_area, width, height);
    gtk_widget_show(drawing_area);
    
    return (NGHandle)drawing_area;
}

void ng_platform_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

// ... rest of Linux/GTK implementation ... 