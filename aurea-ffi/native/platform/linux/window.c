#include "window.h"
#include "utils.h"
#include "menu.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <gdk/gdkkeysyms.h>
#include <string.h>
#if defined(GDK_WINDOWING_X11) && defined(AUREA_HAVE_X11_XCB)
#include <gdk/gdkx.h>
#include <X11/Xlib-xcb.h>
#endif
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

static gboolean g_lifecycle_callbacks[256] = {0};
static GtkWidget* g_lifecycle_windows[256] = {0};
static int g_lifecycle_callback_count = 0;

static const char* AUREA_MAIN_VBOX_KEY = "aurea-main-vbox";

static GtkWidget* ng_linux_window_main_vbox(GtkWidget* window) {
    if (!window) return NULL;
    GtkWidget* vbox = GTK_WIDGET(g_object_get_data(G_OBJECT(window), AUREA_MAIN_VBOX_KEY));
    if (vbox) return vbox;

    GtkContainer* container = GTK_CONTAINER(window);
    GList* children = gtk_container_get_children(container);
    if (children && children->data) {
        vbox = GTK_WIDGET(children->data);
        g_object_set_data(G_OBJECT(window), AUREA_MAIN_VBOX_KEY, vbox);
    }
    if (children) {
        g_list_free(children);
    }
    return vbox;
}

static gboolean on_key_press(GtkWidget* widget, GdkEventKey* event, gpointer user_data);
static gboolean on_key_release(GtkWidget* widget, GdkEventKey* event, gpointer user_data);
static gboolean on_button_press(GtkWidget* widget, GdkEventButton* event, gpointer user_data);
static gboolean on_button_release(GtkWidget* widget, GdkEventButton* event, gpointer user_data);
static gboolean on_motion_notify(GtkWidget* widget, GdkEventMotion* event, gpointer user_data);
static gboolean on_scroll(GtkWidget* widget, GdkEventScroll* event, gpointer user_data);
static gboolean on_focus_in(GtkWidget* widget, GdkEventFocus* event, gpointer user_data);
static gboolean on_focus_out(GtkWidget* widget, GdkEventFocus* event, gpointer user_data);
static gboolean on_enter(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data);
static gboolean on_leave(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data);

static void on_window_destroy(GtkWidget* widget, gpointer data) {
    // Invoke lifecycle callback if enabled
    for (int i = 0; i < g_lifecycle_callback_count; i++) {
        if (g_lifecycle_windows[i] == widget && g_lifecycle_callbacks[i]) {
            ng_invoke_lifecycle_callback((void*)widget, 5); // WindowWillClose = 5
            break;
        }
    }
    /* End the loop only when the last window goes.
     *
     * Quitting on any window's destroy would take every other window with it,
     * and poll_events drives the context directly without ever entering
     * gtk_main, so there is often no loop to quit at all. */
    if (gtk_main_level() == 0) return;

    GList* toplevels = gtk_window_list_toplevels();
    int remaining = 0;
    for (GList* item = toplevels; item != NULL; item = item->next) {
        GtkWidget* other = GTK_WIDGET(item->data);
        if (other != widget && gtk_widget_get_visible(other)) remaining++;
    }
    g_list_free(toplevels);

    if (remaining == 0) gtk_main_quit();
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
    GtkWidget* main_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), main_vbox);
    g_object_set_data(G_OBJECT(window), AUREA_MAIN_VBOX_KEY, main_vbox);
    
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

static struct {
    GtkWidget* window;
    ScaleFactorCallback callback;
} g_scale_callbacks[256] = {0};
static int g_scale_callback_count = 0;

static int g_last_x[256] = {0};
static int g_last_y[256] = {0};
static int g_last_w[256] = {0};
static int g_last_h[256] = {0};
static int g_cursor_grab_mode[256] = {0};
static double g_last_mouse_x[256] = {0.0};
static double g_last_mouse_y[256] = {0.0};
static int g_last_mouse_valid[256] = {0};

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

    /* Punctuation, matched by character so either the base or shifted symbol on
     * the same physical key resolves to the same keycode. */
    switch (ch) {
        case '-': case '_': return NG_KEY_MINUS;
        case '=': case '+': return NG_KEY_EQUALS;
        case '[': case '{': return NG_KEY_LEFT_BRACKET;
        case ']': case '}': return NG_KEY_RIGHT_BRACKET;
        case '\\': case '|': return NG_KEY_BACKSLASH;
        case ';': case ':': return NG_KEY_SEMICOLON;
        case '\'': case '"': return NG_KEY_APOSTROPHE;
        case '`': case '~': return NG_KEY_GRAVE;
        case ',': case '<': return NG_KEY_COMMA;
        case '.': case '>': return NG_KEY_PERIOD;
        case '/': case '?': return NG_KEY_SLASH;
        default: break;
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

/* X11 numbers: 1 left, 2 middle, 3 right, 4 to 7 the wheel, 8 back and
 * 9 forward. Aurea's are Windows' order, so back and forward are 3 and 4
 * on every platform, and the wheel's numbers are skipped. */
static int ng_linux_mouse_button_from_event(guint button) {
    switch (button) {
        case 1:
            return 0;
        case 3:
            return 1;
        case 2:
            return 2;
        default:
            return button >= 8 ? (int)button - 5 : (int)button;
    }
}

static gboolean on_key_press(GtkWidget* widget, GdkEventKey* event, gpointer user_data) {
    unsigned int mods = ng_linux_modifiers(event->state);
    unsigned int keycode = ng_linux_keycode_from_keyval(event->keyval);
    ng_invoke_key_event((void*)widget, keycode, 1, mods);

    if (ng_linux_handle_menu_shortcut((void*)widget, keycode, mods)) {
        return TRUE;
    }

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

/* Pointer state for one window. It was process-wide, so a button held in
 * one window made another treat its crossings as part of a drag, and
 * clicks in two windows could count as one double click. */
typedef struct {
    unsigned int held;
    guint last_button;
    guint32 last_time;
    double last_x;
    double last_y;
    int count;
} NgLinuxPointer;

static NgLinuxPointer* ng_linux_pointer(GtkWidget* window) {
    NgLinuxPointer* pointer = g_object_get_data(G_OBJECT(window), "aurea-pointer");
    if (!pointer) {
        pointer = g_new0(NgLinuxPointer, 1);
        g_object_set_data_full(G_OBJECT(window), "aurea-pointer", pointer, g_free);
    }
    return pointer;
}

/* GTK hands a toplevel its own button and motion events twice: once from
 * _gtk_window_check_handle_wm_event, and once more through normal propagation
 * when the handler lets it through. Report each native event once. */
static gboolean ng_linux_already_seen(GdkEvent* event) {
    static GdkEvent* last = NULL;
    static GdkEventType last_type = GDK_NOTHING;
    static guint32 last_time = 0;
    guint32 time = gdk_event_get_time(event);
    if (event == last && event->type == last_type && time == last_time) {
        return TRUE;
    }
    last = event;
    last_type = event->type;
    last_time = time;
    return FALSE;
}

/* The content widget: whatever set_window_content packed, below any menu bar. */
static GtkWidget* ng_linux_window_content(GtkWidget* window) {
    GtkWidget* vbox = ng_linux_window_main_vbox(window);
    if (!vbox) return NULL;
    GtkWidget* content = vbox;
    GList* children = gtk_container_get_children(GTK_CONTAINER(vbox));
    for (GList* item = children; item != NULL; item = item->next) {
        if (!GTK_IS_MENU_BAR(item->data)) {
            content = GTK_WIDGET(item->data);
            break;
        }
    }
    g_list_free(children);
    return content;
}

/* Pointer positions go out relative to the content, like on Windows and
 * macOS. GTK gives them relative to the GdkWindow the event came in on,
 * and the toplevel's one also holds the CSD shadow, the headerbar and the
 * menu bar. */
static void ng_linux_content_point(GtkWidget* window, GdkWindow* from, double* x, double* y) {
    GdkWindow* top = gtk_widget_get_window(window);
    while (from && from != top) {
        gdk_window_coords_to_parent(from, *x, *y, x, y);
        from = gdk_window_get_parent(from);
    }
    GtkWidget* content = ng_linux_window_content(window);
    int ox = 0;
    int oy = 0;
    if (content && gtk_widget_translate_coordinates(content, window, 0, 0, &ox, &oy)) {
        *x -= ox;
        *y -= oy;
    }
}

/* GTK reports a double click as a second GDK_BUTTON_PRESS and then one more
 * GDK_2BUTTON_PRESS on top. Count clicks on the plain press, with GTK's own
 * double click time and distance, so each click is one press. */
static int ng_linux_click_count(GtkWidget* widget, GdkEventButton* event) {
    NgLinuxPointer* p = ng_linux_pointer(widget);

    gint time = 400;
    gint distance = 5;
    g_object_get(gtk_widget_get_settings(widget),
        "gtk-double-click-time", &time,
        "gtk-double-click-distance", &distance,
        NULL);
    double dx = event->x_root - p->last_x;
    double dy = event->y_root - p->last_y;
    gboolean again = p->count > 0 && event->button == p->last_button &&
        event->time - p->last_time <= (guint32)time &&
        dx * dx + dy * dy <= (double)(distance * distance);

    p->count = again ? p->count + 1 : 1;
    p->last_button = event->button;
    p->last_time = event->time;
    p->last_x = event->x_root;
    p->last_y = event->y_root;
    return p->count;
}

static gboolean on_button_press(GtkWidget* widget, GdkEventButton* event, gpointer user_data) {
    if (event->type != GDK_BUTTON_PRESS) return FALSE;
    if (ng_linux_already_seen((GdkEvent*)event)) return FALSE;
    unsigned int mods = ng_linux_modifiers(event->state);
    int button = ng_linux_mouse_button_from_event(event->button);
    int click_count = ng_linux_click_count(widget, event);
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    if (button < 8) ng_linux_pointer(widget)->held |= 1u << button;
    ng_invoke_mouse_button((void*)widget, button, 1, mods, x, y, click_count);
    return FALSE;
}

static gboolean on_button_release(GtkWidget* widget, GdkEventButton* event, gpointer user_data) {
    if (ng_linux_already_seen((GdkEvent*)event)) return FALSE;
    unsigned int mods = ng_linux_modifiers(event->state);
    int button = ng_linux_mouse_button_from_event(event->button);
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    if (button < 8) ng_linux_pointer(widget)->held &= ~(1u << button);
    ng_invoke_mouse_button((void*)widget, button, 0, mods, x, y, 1);
    return FALSE;
}

static gboolean on_motion_notify(GtkWidget* widget, GdkEventMotion* event, gpointer user_data) {
    if (ng_linux_already_seen((GdkEvent*)event)) return FALSE;
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    unsigned int buttons = 0;
    if (event->state & GDK_BUTTON1_MASK) buttons |= 1u << 0;
    if (event->state & GDK_BUTTON3_MASK) buttons |= 1u << 1;
    if (event->state & GDK_BUTTON2_MASK) buttons |= 1u << 2;
    /* The state has no bits for back, forward and beyond. */
    buttons |= ng_linux_pointer(widget)->held & ~7u;
    ng_invoke_mouse_move((void*)widget, x, y, buttons, ng_linux_modifiers(event->state));

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

    /* GDK counts down as positive; Aurea, like Windows and AppKit, up. */
    ng_invoke_mouse_wheel((void*)widget, dx, -dy, mods);
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

/* Crossing into or out of one of the window's own children, like a canvas
 * with its own GdkWindow, is still inside the window. So is anything while a
 * button is held: GDK moves its implicit grab on press and sends the toplevel
 * a leave for it, and a drag that goes outside is not a leave until the
 * release. */
static gboolean ng_linux_crossing_is_inside(GtkWidget* widget, GdkEventCrossing* event) {
    return ng_linux_pointer(widget)->held != 0 ||
        event->detail == GDK_NOTIFY_INFERIOR ||
        event->window != gtk_widget_get_window(widget);
}

static gboolean on_enter(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data) {
    if (ng_linux_crossing_is_inside(widget, event)) return FALSE;
    ng_invoke_cursor_entered((void*)widget, 1);
    return FALSE;
}

static gboolean on_leave(GtkWidget* widget, GdkEventCrossing* event, gpointer user_data) {
    if (ng_linux_crossing_is_inside(widget, event)) return FALSE;
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
    GtkWidget* widget = (GtkWidget*)handle;
    /* The window may already be gone: closing one from the window manager
       destroys the widget, and the Rust value that owns it is dropped later
       and asks again. Checking first is what ng_linux_destroy_element does,
       and skips a destroy of something that is no longer there. */
    if (!GTK_IS_WIDGET(widget)) return;
    gtk_widget_destroy(widget);
}

void ng_linux_destroy_element(NGHandle element) {
    if (!element) return;
    GtkWidget* widget = (GtkWidget*)element;
    if (!GTK_IS_WIDGET(widget)) return;
    /* Destroys the widget and everything inside it, so a container frees its
       children — which is why the Rust side hands ownership over when a child
       is added, rather than freeing it twice. */
    gtk_widget_destroy(widget);
}

int ng_linux_detach_element(NGHandle element) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* widget = (GtkWidget*)element;
    if (!GTK_IS_WIDGET(widget)) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* parent = gtk_widget_get_parent(widget);
    if (!parent) return NG_SUCCESS;
    if (!GTK_IS_CONTAINER(parent)) return NG_ERROR_PLATFORM_SPECIFIC;

    /* gtk_container_remove drops the parent's reference, which would take the
       widget with it. Hold one across the removal and hand it back to the
       caller, who owns the widget again once it is unparented. */
    g_object_ref(widget);
    gtk_container_remove(GTK_CONTAINER(parent), widget);
    return NG_SUCCESS;
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
    
    GtkWidget* vbox = ng_linux_window_main_vbox(window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;

    // Remove previous content widgets while keeping menu bars.
    GList* children = gtk_container_get_children(GTK_CONTAINER(vbox));
    for (GList* item = children; item != NULL; item = item->next) {
        GtkWidget* child = GTK_WIDGET(item->data);
        if (!GTK_IS_MENU_BAR(child)) {
            gtk_container_remove(GTK_CONTAINER(vbox), child);
        }
    }
    if (children) {
        g_list_free(children);
    }

    gtk_box_pack_end(GTK_BOX(vbox), content, TRUE, TRUE, 0);
    gtk_widget_show_all(window);

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

NGHandle ng_linux_window_get_content_view(NGHandle window) {
    return (NGHandle)ng_linux_window_main_vbox((GtkWidget*)window);
}

void ng_linux_window_set_title(NGHandle window, const char* title) {
    if (!window || !title) return;
    GtkWidget* widget = (GtkWidget*)window;
    gtk_window_set_title(GTK_WINDOW(widget), title);
}

int ng_linux_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
) {
    if (!window || !rgba || width == 0 || height == 0) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    GdkPixbuf* pixbuf = gdk_pixbuf_new(
        GDK_COLORSPACE_RGB,
        TRUE,
        8,
        (int)width,
        (int)height
    );
    if (!pixbuf) return NG_ERROR_PLATFORM_SPECIFIC;

    unsigned char* pixels = gdk_pixbuf_get_pixels(pixbuf);
    int row_stride = gdk_pixbuf_get_rowstride(pixbuf);
    size_t source_stride = (size_t)width * 4;
    for (unsigned int row = 0; row < height; ++row) {
        memcpy(
            pixels + (size_t)row * (size_t)row_stride,
            rgba + (size_t)row * source_stride,
            source_stride
        );
    }
    gtk_window_set_icon(GTK_WINDOW((GtkWidget*)window), pixbuf);
    g_object_unref(pixbuf);
    return NG_SUCCESS;
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
#if defined(GDK_WINDOWING_X11) && defined(AUREA_HAVE_X11_XCB)
    if (GDK_IS_X11_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display || !GDK_IS_X11_DISPLAY(gdk_display)) return 0;
        Display* xdisplay = gdk_x11_display_get_xdisplay(gdk_display);
        xcb_connection_t* connection = xdisplay ? XGetXCBConnection(xdisplay) : NULL;
        Window xid = gdk_x11_window_get_xid(gdk_window);
        if (!connection || xid == 0 || xid > UINT32_MAX) return 0;
        *xcb_window = (uint32_t)xid;
        *xcb_connection = connection;
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

int ng_linux_window_set_cursor_icon(NGHandle window, int icon) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    GdkWindow* gdkWindow = gtk_widget_get_window((GtkWidget*)window);
    if (!gdkWindow) return NG_ERROR_INVALID_HANDLE;

    static const char* const names[] = {
        NULL, "pointer", "text", "crosshair", "move",
        "ew-resize", "ns-resize", "not-allowed", "wait",
    };
    int count = (int)(sizeof(names) / sizeof(names[0]));
    const char* name = (icon > 0 && icon < count) ? names[icon] : NULL;
    if (!name) {
        gdk_window_set_cursor(gdkWindow, NULL);
        return NG_SUCCESS;
    }

    GdkCursor* cursor = gdk_cursor_new_from_name(gdk_window_get_display(gdkWindow), name);
    if (!cursor) return NG_ERROR_PLATFORM_SPECIFIC;
    gdk_window_set_cursor(gdkWindow, cursor);
    g_object_unref(cursor);
    return NG_SUCCESS;
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

char* ng_linux_get_clipboard_text(void) {
    GtkClipboard* cb = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    if (!cb) return NULL;
    return gtk_clipboard_wait_for_text(cb);
}

void ng_linux_free_clipboard_text(char* text) {
    if (text) g_free(text);
}

int ng_linux_set_clipboard_text(const char* text) {
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    GtkClipboard* cb = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    if (!cb) return NG_ERROR_PLATFORM_SPECIFIC;
    gtk_clipboard_set_text(cb, text, -1);
    return NG_SUCCESS;
}
