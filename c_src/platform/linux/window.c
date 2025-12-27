#include "window.h"
#include "utils.h"
#include "common/errors.h"
#include <gtk/gtk.h>

static GtkWidget* main_vbox = NULL;

static void on_window_destroy(GtkWidget* widget, gpointer data) {
    // Invoke lifecycle callback if enabled
    for (int i = 0; i < g_lifecycle_callback_count; i++) {
        if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
            ng_invoke_lifecycle_callback((void*)widget, 5); // WindowWillClose = 5
            break;
        }
    }
    // Quit the GTK main loop when window is closed
    gtk_main_quit();
}

static gboolean on_window_state_event(GtkWidget* widget, GdkEventWindowState* event, gpointer user_data) {
    if (event->changed_mask & GDK_WINDOW_STATE_ICONIFIED) {
        if (event->new_window_state & GDK_WINDOW_STATE_ICONIFIED) {
            // Window minimized
            for (int i = 0; i < g_lifecycle_callback_count; i++) {
                if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)widget, 6); // WindowMinimized = 6
                    break;
                }
            }
        } else {
            // Window restored
            for (int i = 0; i < g_lifecycle_callback_count; i++) {
                if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)widget, 7); // WindowRestored = 7
                    break;
                }
            }
        }
    }
    return FALSE;
}

NGHandle ng_linux_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), title);
    gtk_window_set_default_size(GTK_WINDOW(window), width, height);
    
    // Connect destroy signal to quit the event loop
    g_signal_connect(G_OBJECT(window), "destroy", G_CALLBACK(on_window_destroy), NULL);
    // Connect window state events for minimize/restore
    g_signal_connect(G_OBJECT(window), "window-state-event", G_CALLBACK(on_window_state_event), NULL);
    
    // Create a vertical box to hold menu and content
    main_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), main_vbox);
    
    gtk_widget_show_all(window);
    
    return (NGHandle)window;
}

float ng_linux_get_scale_factor(NGHandle window) {
    if (!window) return 1.0f;
    GtkWindow* gtkWindow = (GtkWindow*)window;
    GdkWindow* gdkWindow = gtk_widget_get_window(GTK_WIDGET(gtkWindow));
    if (gdkWindow) {
        gint scale = gdk_window_get_scale_factor(gdkWindow);
        return (float)scale;
    }
    return 1.0f;
}

typedef void (*ScaleFactorCallback)(void*, float);

static struct {
    GtkWidget* window;
    ScaleFactorCallback callback;
} g_scale_callbacks[256] = {0};
static int g_scale_callback_count = 0;

static gboolean g_lifecycle_callbacks[256] = {0};
static GtkWidget* g_lifecycle_windows[256] = {0};
static int g_lifecycle_callback_count = 0;

extern void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);

static gboolean on_configure_event(GtkWidget* widget, GdkEventConfigure* event, gpointer user_data) {
    // Check for scale factor changes
    GdkWindow* gdkWindow = gtk_widget_get_window(widget);
    if (gdkWindow) {
        gint scale = gdk_window_get_scale_factor(gdkWindow);
        float scale_factor = (float)scale;
        
        for (int i = 0; i < g_scale_callback_count; i++) {
            if (g_scale_callbacks[i].window == widget && g_scale_callbacks[i].callback) {
                g_scale_callbacks[i].callback((void*)widget, scale_factor);
                break;
            }
        }
    }
    return FALSE;
}

void ng_linux_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    GtkWidget* widget = (GtkWidget*)window;
    
    // Find or add entry
    int found = -1;
    for (int i = 0; i < g_scale_callback_count; i++) {
        if (g_scale_callbacks[i].window == widget) {
            found = i;
            break;
        }
    }
    
    if (found >= 0) {
        g_scale_callbacks[found].callback = callback;
    } else if (g_scale_callback_count < 256) {
        g_scale_callbacks[g_scale_callback_count].window = widget;
        g_scale_callbacks[g_scale_callback_count].callback = callback;
        g_signal_connect(G_OBJECT(widget), "configure-event", G_CALLBACK(on_configure_event), NULL);
        g_scale_callback_count++;
    }
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

void ng_linux_window_set_lifecycle_callback(NGHandle window) {
    if (!window) return;
    GtkWidget* widget = (GtkWidget*)window;
    
    // Find or add entry
    int found = -1;
    for (int i = 0; i < g_lifecycle_callback_count; i++) {
        if (g_lifecycle_windows[i] == widget) {
            found = i;
            break;
        }
    }
    
    if (found >= 0) {
        g_lifecycle_callbacks[found] = TRUE;
    } else if (g_lifecycle_callback_count < 256) {
        g_lifecycle_windows[g_lifecycle_callback_count] = widget;
        g_lifecycle_callbacks[g_lifecycle_callback_count] = TRUE;
        g_lifecycle_callback_count++;
    }
}

void* ng_linux_get_main_vbox(void) {
    return (void*)main_vbox;
}

