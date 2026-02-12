#include "window.h"
#include "utils.h"
#include "common/errors.h"
#include "common/input.h"
#include <gtk/gtk.h>
#include <gdk/gdkkeysyms.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

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
                    ng_invoke_lifecycle_callback((void*)widget, 9); // SurfaceLost = 9
                    break;
                }
            }
        } else {
            // Window restored
            for (int i = 0; i < g_lifecycle_callback_count; i++) {
                if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
                    ng_invoke_lifecycle_callback((void*)widget, 7); // WindowRestored = 7
                    ng_invoke_lifecycle_callback((void*)widget, 10); // SurfaceRecreated = 10
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

    gtk_widget_add_events(
        window,
        GDK_KEY_PRESS_MASK | GDK_KEY_RELEASE_MASK | GDK_BUTTON_PRESS_MASK |
            GDK_BUTTON_RELEASE_MASK | GDK_POINTER_MOTION_MASK | GDK_SCROLL_MASK |
            GDK_ENTER_NOTIFY_MASK | GDK_LEAVE_NOTIFY_MASK | GDK_FOCUS_CHANGE_MASK);

    g_signal_connect(G_OBJECT(window), "key-press-event", G_CALLBACK(on_key_press), NULL);
    g_signal_connect(G_OBJECT(window), "key-release-event", G_CALLBACK(on_key_release), NULL);
    g_signal_connect(G_OBJECT(window), "button-press-event", G_CALLBACK(on_button_press), NULL);
    g_signal_connect(G_OBJECT(window), "button-release-event", G_CALLBACK(on_button_release), NULL);
    g_signal_connect(G_OBJECT(window), "motion-notify-event", G_CALLBACK(on_motion_notify), NULL);
    g_signal_connect(G_OBJECT(window), "scroll-event", G_CALLBACK(on_scroll), NULL);
    g_signal_connect(G_OBJECT(window), "focus-in-event", G_CALLBACK(on_focus_in), NULL);
    g_signal_connect(G_OBJECT(window), "focus-out-event", G_CALLBACK(on_focus_out), NULL);
    g_signal_connect(G_OBJECT(window), "enter-notify-event", G_CALLBACK(on_enter), NULL);
    g_signal_connect(G_OBJECT(window), "leave-notify-event", G_CALLBACK(on_leave), NULL);
    
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

NGHandle ng_linux_create_window_with_type(const char* title, int width, int height, int window_type) {
    NGHandle handle = ng_linux_create_window(title, width, height);
    if (!handle) return NULL;

    GtkWidget* widget = (GtkWidget*)handle;
    GdkWindowTypeHint hint = GDK_WINDOW_TYPE_HINT_NORMAL;
    switch (window_type) {
        case 1: // Popup
            hint = GDK_WINDOW_TYPE_HINT_POPUP_MENU;
            break;
        case 2: // Tool
            hint = GDK_WINDOW_TYPE_HINT_TOOLBAR;
            break;
        case 3: // Utility
            hint = GDK_WINDOW_TYPE_HINT_UTILITY;
            break;
        case 4: // Sheet
        case 5: // Dialog
            hint = GDK_WINDOW_TYPE_HINT_DIALOG;
            break;
        default:
            hint = GDK_WINDOW_TYPE_HINT_NORMAL;
            break;
    }
    gtk_window_set_type_hint(GTK_WINDOW(widget), hint);
    return handle;
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
static int g_last_x[256] = {0};
static int g_last_y[256] = {0};
static int g_last_w[256] = {0};
static int g_last_h[256] = {0};
static int g_cursor_grab_mode[256] = {0};
static double g_last_mouse_x[256] = {0.0};
static double g_last_mouse_y[256] = {0.0};
static int g_last_mouse_valid[256] = {0};

extern void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);
extern void ng_invoke_key_event(void* window, unsigned int keycode, int pressed, unsigned int modifiers);
extern void ng_invoke_mouse_button(void* window, int button, int pressed, unsigned int modifiers);
extern void ng_invoke_mouse_move(void* window, double x, double y);
extern void ng_invoke_mouse_wheel(void* window, double delta_x, double delta_y, unsigned int modifiers);
extern void ng_invoke_text_input(void* window, const char* text);
extern void ng_invoke_focus_changed(void* window, int focused);
extern void ng_invoke_cursor_entered(void* window, int entered);
extern void ng_invoke_raw_mouse_motion(void* window, double delta_x, double delta_y);

static int ng_linux_find_window_index(GtkWidget* widget) {
    for (int i = 0; i < g_lifecycle_callback_count; i++) {
        if (g_lifecycle_windows[i] == widget) {
            return i;
        }
    }
    return -1;
}

static unsigned int ng_linux_modifiers(GdkModifierType state) {
    unsigned int mods = 0;
    if (state & GDK_SHIFT_MASK) {
        mods |= NG_MOD_SHIFT;
    }
    if (state & GDK_CONTROL_MASK) {
        mods |= NG_MOD_CTRL;
    }
    if (state & GDK_MOD1_MASK) {
        mods |= NG_MOD_ALT;
    }
    if (state & GDK_SUPER_MASK || state & GDK_META_MASK) {
        mods |= NG_MOD_META;
    }
    return mods;
}

static unsigned int ng_linux_keycode_from_keyval(guint keyval) {
    gunichar ch = gdk_keyval_to_unicode(gdk_keyval_to_upper(keyval));
    if (ch >= 'A' && ch <= 'Z') {
        return NG_KEY_A + (unsigned int)(ch - 'A');
    }
    if (ch >= '0' && ch <= '9') {
        return NG_KEY_0 + (unsigned int)(ch - '0');
    }

    switch (keyval) {
        case GDK_KEY_space:
            return NG_KEY_SPACE;
        case GDK_KEY_Return:
        case GDK_KEY_KP_Enter:
            return NG_KEY_ENTER;
        case GDK_KEY_Escape:
            return NG_KEY_ESCAPE;
        case GDK_KEY_Tab:
            return NG_KEY_TAB;
        case GDK_KEY_BackSpace:
            return NG_KEY_BACKSPACE;
        case GDK_KEY_Delete:
            return NG_KEY_DELETE;
        case GDK_KEY_Insert:
            return NG_KEY_INSERT;
        case GDK_KEY_Home:
            return NG_KEY_HOME;
        case GDK_KEY_End:
            return NG_KEY_END;
        case GDK_KEY_Page_Up:
            return NG_KEY_PAGE_UP;
        case GDK_KEY_Page_Down:
            return NG_KEY_PAGE_DOWN;
        case GDK_KEY_Up:
            return NG_KEY_UP;
        case GDK_KEY_Down:
            return NG_KEY_DOWN;
        case GDK_KEY_Left:
            return NG_KEY_LEFT;
        case GDK_KEY_Right:
            return NG_KEY_RIGHT;
        case GDK_KEY_F1:
            return NG_KEY_F1;
        case GDK_KEY_F2:
            return NG_KEY_F2;
        case GDK_KEY_F3:
            return NG_KEY_F3;
        case GDK_KEY_F4:
            return NG_KEY_F4;
        case GDK_KEY_F5:
            return NG_KEY_F5;
        case GDK_KEY_F6:
            return NG_KEY_F6;
        case GDK_KEY_F7:
            return NG_KEY_F7;
        case GDK_KEY_F8:
            return NG_KEY_F8;
        case GDK_KEY_F9:
            return NG_KEY_F9;
        case GDK_KEY_F10:
            return NG_KEY_F10;
        case GDK_KEY_F11:
            return NG_KEY_F11;
        case GDK_KEY_F12:
            return NG_KEY_F12;
        case GDK_KEY_Shift_L:
        case GDK_KEY_Shift_R:
            return NG_KEY_SHIFT;
        case GDK_KEY_Control_L:
        case GDK_KEY_Control_R:
            return NG_KEY_CONTROL;
        case GDK_KEY_Alt_L:
        case GDK_KEY_Alt_R:
            return NG_KEY_ALT;
        case GDK_KEY_Super_L:
        case GDK_KEY_Super_R:
        case GDK_KEY_Meta_L:
        case GDK_KEY_Meta_R:
            return NG_KEY_META;
        default:
            return NG_KEY_UNKNOWN;
    }
}

static int ng_linux_mouse_button_from_event(guint button) {
    switch (button) {
        case 1:
            return 0;
        case 3:
            return 1;
        case 2:
            return 2;
        default:
            return (int)button;
    }
}

static gboolean on_key_press(GtkWidget* widget, GdkEventKey* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    unsigned int keycode = ng_linux_keycode_from_keyval(event->keyval);
    ng_invoke_key_event((void*)widget, keycode, 1, mods);

    if (event->string && event->string[0] != '\0') {
        ng_invoke_text_input((void*)widget, event->string);
    }
    return FALSE;
}

static gboolean on_key_release(GtkWidget* widget, GdkEventKey* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    unsigned int keycode = ng_linux_keycode_from_keyval(event->keyval);
    ng_invoke_key_event((void*)widget, keycode, 0, mods);
    return FALSE;
}

static gboolean on_button_press(GtkWidget* widget, GdkEventButton* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    int button = ng_linux_mouse_button_from_event(event->button);
    ng_invoke_mouse_button((void*)widget, button, 1, mods);
    return FALSE;
}

static gboolean on_button_release(GtkWidget* widget, GdkEventButton* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    int button = ng_linux_mouse_button_from_event(event->button);
    ng_invoke_mouse_button((void*)widget, button, 0, mods);
    return FALSE;
}

static gboolean on_motion_notify(GtkWidget* widget, GdkEventMotion* event, gpointer user_data) {
    ng_invoke_mouse_move((void*)widget, event->x, event->y);

    int index = ng_linux_find_window_index(widget);
    if (index >= 0 && g_cursor_grab_mode[index] == 2) {
        if (!g_last_mouse_valid[index]) {
            g_last_mouse_x[index] = event->x;
            g_last_mouse_y[index] = event->y;
            g_last_mouse_valid[index] = 1;
        } else {
            double dx = event->x - g_last_mouse_x[index];
            double dy = event->y - g_last_mouse_y[index];
            g_last_mouse_x[index] = event->x;
            g_last_mouse_y[index] = event->y;
            ng_invoke_raw_mouse_motion((void*)widget, dx, dy);
        }
    }
    return FALSE;
}

static gboolean on_scroll(GtkWidget* widget, GdkEventScroll* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    double dx = 0.0;
    double dy = 0.0;

    if (!gdk_event_get_scroll_deltas((GdkEvent*)event, &dx, &dy)) {
        switch (event->direction) {
            case GDK_SCROLL_UP:
                dy = -1.0;
                break;
            case GDK_SCROLL_DOWN:
                dy = 1.0;
                break;
            case GDK_SCROLL_LEFT:
                dx = -1.0;
                break;
            case GDK_SCROLL_RIGHT:
                dx = 1.0;
                break;
            default:
                break;
        }
    }

    ng_invoke_mouse_wheel((void*)widget, dx, dy, mods);
    return FALSE;
}

static gboolean on_focus_in(GtkWidget* widget, GdkEventFocus* event, gpointer user_data) {
    ng_invoke_focus_changed((void*)widget, 1);
    return FALSE;
}

static gboolean on_focus_out(GtkWidget* widget, GdkEventFocus* event, gpointer user_data) {
    ng_invoke_focus_changed((void*)widget, 0);
    return FALSE;
}

static gboolean on_enter(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data) {
    ng_invoke_cursor_entered((void*)widget, 1);
    return FALSE;
}

static gboolean on_leave(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data) {
    ng_invoke_cursor_entered((void*)widget, 0);
    return FALSE;
}

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
    for (int i = 0; i < g_lifecycle_callback_count; i++) {
        if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
            if (g_last_x[i] != event->x || g_last_y[i] != event->y) {
                g_last_x[i] = event->x;
                g_last_y[i] = event->y;
                ng_invoke_lifecycle_callback((void*)widget, 11); // WindowMoved = 11
            }
            if (g_last_w[i] != event->width || g_last_h[i] != event->height) {
                g_last_w[i] = event->width;
                g_last_h[i] = event->height;
                ng_invoke_lifecycle_callback((void*)widget, 12); // WindowResized = 12
            }
            break;
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

void ng_linux_window_show(NGHandle window) {
    if (!window) return;
    gtk_widget_show(GTK_WIDGET(window));
}

void ng_linux_window_hide(NGHandle window) {
    if (!window) return;
    gtk_widget_hide(GTK_WIDGET(window));
}

int ng_linux_window_is_visible(NGHandle window) {
    if (!window) return 0;
    return gtk_widget_get_visible(GTK_WIDGET(window)) ? 1 : 0;
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
        int index = g_lifecycle_callback_count;
        g_lifecycle_windows[index] = widget;
        g_lifecycle_callbacks[index] = TRUE;
        gtk_window_get_position(GTK_WINDOW(widget), &g_last_x[index], &g_last_y[index]);
        gtk_window_get_size(GTK_WINDOW(widget), &g_last_w[index], &g_last_h[index]);
        g_cursor_grab_mode[index] = 0;
        g_last_mouse_valid[index] = 0;
        g_lifecycle_callback_count++;
    }
}

void* ng_linux_get_main_vbox(void) {
    return (void*)main_vbox;
}

void ng_linux_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_set_title(GTK_WINDOW(widget), title);
}

void ng_linux_window_set_size(NGHandle window, int width, int height) {
    if (!window) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_resize(GTK_WINDOW(widget), width, height);
}

void ng_linux_window_get_size(NGHandle window, int* width, int* height) {
    if (!window || !width || !height) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_get_size(GTK_WINDOW(widget), width, height);
}

void ng_linux_window_set_position(NGHandle window, int x, int y) {
    if (!window) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_move(GTK_WINDOW(widget), x, y);
}

void ng_linux_window_get_position(NGHandle window, int* x, int* y) {
    if (!window || !x || !y) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_get_position(GTK_WINDOW(widget), x, y);
}

void ng_linux_window_request_close(NGHandle window) {
    if (!window) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_close(GTK_WINDOW(widget));
}

int ng_linux_window_is_focused(NGHandle window) {
    if (!window) return 0;
    GtkWidget* widget = (GtkWidget*)window;
    return gtk_window_is_active(GTK_WINDOW(widget)) ? 1 : 0;
}

int ng_linux_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection) {
    if (!window || !xcb_window || !xcb_connection) return 0;
    GtkWidget* widget = (GtkWidget*)window;
    GdkWindow* gdk_window = gtk_widget_get_window(widget);
    if (!gdk_window) return 0;
#ifdef GDK_WINDOWING_X11
    if (GDK_IS_X11_WINDOW(gdk_window)) {
        GdkDisplay* display = gdk_window_get_display(gdk_window);
        if (!display) return 0;
        *xcb_window = (uint32_t)gdk_x11_window_get_xid(gdk_window);
        *xcb_connection = gdk_x11_display_get_xcb_connection(display);
        if (*xcb_window == 0 || *xcb_connection == NULL) {
            return 0;
        }
        return 1;
    }
#endif
    return 0;
}

int ng_linux_window_get_wayland_handle(NGHandle window, void** surface, void** display) {
    if (!window || !surface || !display) return 0;
    GtkWidget* widget = (GtkWidget*)window;
    GdkWindow* gdk_window = gtk_widget_get_window(widget);
    if (!gdk_window) return 0;
#ifdef GDK_WINDOWING_WAYLAND
    if (GDK_IS_WAYLAND_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display) return 0;
        *surface = gdk_wayland_window_get_wl_surface(gdk_window);
        *display = gdk_wayland_display_get_wl_display(gdk_display);
        if (*surface == NULL || *display == NULL) {
            return 0;
        }
        return 1;
    }
#endif
    return 0;
}

int ng_linux_window_set_cursor_visible(NGHandle window, int visible) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* widget = (GtkWidget*)window;
    GdkWindow* gdkWindow = gtk_widget_get_window(widget);
    if (!gdkWindow) return NG_ERROR_INVALID_HANDLE;

    if (visible) {
        gdk_window_set_cursor(gdkWindow, NULL);
        return NG_SUCCESS;
    }

    GdkDisplay* display = gdk_window_get_display(gdkWindow);
    if (!display) return NG_ERROR_PLATFORM_SPECIFIC;

    GdkCursor* cursor = gdk_cursor_new_for_display(display, GDK_BLANK_CURSOR);
    if (!cursor) return NG_ERROR_PLATFORM_SPECIFIC;

    gdk_window_set_cursor(gdkWindow, cursor);
    g_object_unref(cursor);

    return NG_SUCCESS;
}

int ng_linux_window_set_cursor_grab(NGHandle window, int mode) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* widget = (GtkWidget*)window;
    GdkWindow* gdkWindow = gtk_widget_get_window(widget);
    if (!gdkWindow) return NG_ERROR_INVALID_HANDLE;

    int index = ng_linux_find_window_index(widget);
    if (index >= 0) {
        g_cursor_grab_mode[index] = mode;
        g_last_mouse_valid[index] = 0;
    }

    GdkDisplay* display = gdk_window_get_display(gdkWindow);
    if (!display) return NG_ERROR_PLATFORM_SPECIFIC;
    GdkSeat* seat = gdk_display_get_default_seat(display);
    if (!seat) return NG_ERROR_PLATFORM_SPECIFIC;

    if (mode == 0) {
        gdk_seat_ungrab(seat);
        return NG_SUCCESS;
    }

    GdkGrabStatus status = gdk_seat_grab(
        seat,
        gdkWindow,
        GDK_SEAT_CAPABILITY_POINTER,
        TRUE,
        NULL,
        NULL,
        NULL,
        NULL);
    if (status != GDK_GRAB_SUCCESS) {
        return NG_ERROR_PLATFORM_SPECIFIC;
    }

    return NG_SUCCESS;
}
