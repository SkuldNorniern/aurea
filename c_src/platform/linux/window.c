#include "window.h"
#include "utils.h"
#include "../common/errors.h"
#include <gtk/gtk.h>

static GtkWidget* main_vbox = NULL;

static void on_window_destroy(GtkWidget* widget, gpointer data) {
    // Quit the GTK main loop when window is closed
    gtk_main_quit();
}

NGHandle ng_linux_create_window(const char* title, int width, int height) {
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

void ng_linux_destroy_window(NGHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

int ng_linux_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!window_handle || !content_handle) return NG_ERROR_INVALID_HANDLE;
    
    GtkWidget* window = (GtkWidget*)window_handle;
    GtkWidget* content = (GtkWidget*)content_handle;
    
    // Get the main vbox from the window
    GtkContainer* container = GTK_CONTAINER(window);
    GList* children = gtk_container_get_children(container);
    
    if (children && children->data) {
        GtkWidget* vbox = (GtkWidget*)children->data;
        gtk_container_add(GTK_CONTAINER(vbox), content);
        gtk_widget_show_all(window);
    }
    
    if (children) {
        g_list_free(children);
    }
    
    return NG_SUCCESS;
}

void* ng_linux_get_main_vbox(void) {
    return (void*)main_vbox;
}

