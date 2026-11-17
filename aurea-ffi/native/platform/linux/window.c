#include "window.h"
#include "utils.h"
#ifdef NG_FEATURE_MENU
#include "menu.h"
#include "common/menu_table.h"
#endif
#include "ime.h"
#include "control.h"
#include "common/shortcuts.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include "common/types.h"
#include <gtk/gtk.h>
#include <gdk/gdkkeysyms.h>
#include <limits.h>
#include <string.h>
#if defined(GDK_WINDOWING_X11) && defined(AUREA_HAVE_X11_XCB)
#include <gdk/gdkx.h>
#include <X11/Xlib-xcb.h>
#endif
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

/* What a gesture of the pointer has built up so far on a window. */
typedef struct {
    unsigned int held;
    guint last_button;
    guint32 last_time;
    double last_x;
    double last_y;
    int count;
} NgLinuxPointer;

/* The last pointer event a window handled; a repeat of it is dropped. */
typedef struct {
    gboolean valid;
    GdkEventType type;
    guint32 time;
    guint button;
    double x;
    double y;
} NgLinuxLastEvent;

/* What Aurea keeps for one GTK window; freed with the window. */
typedef struct {
    ScaleFactorCallback on_scale;
    gboolean lifecycle;
    gboolean close_guard;
    /* The frame's place and size as last reported. */
    int x, y, width, height;
    /* The rect kept while not maximized, tiled, fullscreen or iconified. */
    int restored[4];
    gboolean has_restored;
    GtkWidget* main_vbox;
    /* Where the pointer last was, for relative motion while grabbed. */
    int grab_mode;
    gboolean mouse_known;
    double mouse_x, mouse_y;
    /* A key a menu shortcut took, so its release is swallowed too. */
    guint menu_took_key;
    NgLinuxPointer pointer;
    NgLinuxLastEvent last;
    /* The file drag over the window: where it is, and whether it is being dropped. */
    gboolean drag_drop;
    int drag_x, drag_y;
    GtkGesture* zoom;
    GtkGesture* rotate;
} NgLinuxWindow;

#define AUREA_WINDOW_KEY "aurea-window"

static void window_free(gpointer data) {
    NgLinuxWindow* w = data;
    g_clear_object(&w->zoom);
    g_clear_object(&w->rotate);
    g_free(w);
}

static NgLinuxWindow* window_state(GtkWidget* window) {
    NgLinuxWindow* w = g_object_get_data(G_OBJECT(window), AUREA_WINDOW_KEY);
    if (!w) {
        w = g_new0(NgLinuxWindow, 1);
        g_object_set_data_full(G_OBJECT(window), AUREA_WINDOW_KEY, w, window_free);
    }
    return w;
}

static GtkWidget* main_vbox_of(GtkWidget* window) {
    return GTK_IS_WINDOW(window) ? window_state(window)->main_vbox : NULL;
}

/* Tells Rust about `widget`'s lifecycle `code`, if it listens for them. */
static void ng_linux_lifecycle(GtkWidget* widget, unsigned int code) {
    if (window_state(widget)->lifecycle) ng_invoke_lifecycle_callback((void*)widget, code);
}

static gboolean on_key_press(GtkWidget* widget, GdkEventKey* event, gpointer data);
static gboolean on_key_release(GtkWidget* widget, GdkEventKey* event, gpointer data);
static gboolean on_button_press(GtkWidget* widget, GdkEventButton* event, gpointer data);
static gboolean on_button_release(GtkWidget* widget, GdkEventButton* event, gpointer data);
static gboolean on_motion_notify(GtkWidget* widget, GdkEventMotion* event, gpointer data);
static gboolean on_scroll(GtkWidget* widget, GdkEventScroll* event, gpointer data);
static gboolean on_configure_event(GtkWidget* widget, GdkEventConfigure* event, gpointer data);
static gboolean on_focus_in(GtkWidget* widget, GdkEventFocus* event, gpointer data);
static gboolean on_focus_out(GtkWidget* widget, GdkEventFocus* event, gpointer data);
static gboolean on_enter(GtkWidget* widget, GdkEventCrossing* event, gpointer data);
static gboolean on_leave(GtkWidget* widget, GdkEventCrossing* event, gpointer data);

static gboolean on_delete_event(GtkWidget* widget, GdkEvent* event, gpointer data) {
    (void)event;
    (void)data;
    if (!window_state(widget)->close_guard) return FALSE;
    ng_linux_lifecycle(widget, 5);
    return TRUE;
}

static void on_window_destroy(GtkWidget* widget, gpointer data) {
    (void)data;
    ng_linux_lifecycle(widget, 5);
#ifdef NG_FEATURE_MENU
    ng_menu_table_forget_window(widget);
#endif
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

#define NG_LINUX_UNSIZED_STATES \
    (GDK_WINDOW_STATE_MAXIMIZED | GDK_WINDOW_STATE_FULLSCREEN | GDK_WINDOW_STATE_TILED | \
     GDK_WINDOW_STATE_ICONIFIED)

static gboolean on_window_state_event(GtkWidget* widget, GdkEventWindowState* event, gpointer data) {
    (void)data;
    if (!(event->changed_mask & GDK_WINDOW_STATE_ICONIFIED)) return FALSE;
    if (event->new_window_state & GDK_WINDOW_STATE_ICONIFIED) {
        ng_linux_lifecycle(widget, 6);
        ng_linux_lifecycle(widget, 9);
    } else {
        ng_linux_lifecycle(widget, 7);
        ng_linux_lifecycle(widget, 10);
    }
    return FALSE;
}

#ifdef NG_INPUT_DROP_FILES
static char* drop_paths(GtkSelectionData* data) {
    gchar** uris = gtk_selection_data_get_uris(data);
    if (!uris) return NULL;
    GString* out = g_string_new(NULL);
    for (gchar** uri = uris; *uri; uri++) {
        gchar* path = g_filename_from_uri(*uri, NULL, NULL);
        if (!path) continue;
        if (out->len) g_string_append_c(out, '\n');
        g_string_append(out, path);
        g_free(path);
    }
    g_strfreev(uris);
    return g_string_free(out, out->len == 0);
}

static void drag_at(GtkWidget* w, gint x, gint y) {
    int ox = 0;
    int oy = 0;
    ng_linux_content_origin(w, &ox, &oy);
    NgLinuxWindow* state = window_state(w);
    state->drag_x = x - ox;
    state->drag_y = y - oy;
}

static gboolean on_drag_motion(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, guint time, gpointer d) {
    (void)d;
    drag_at(w, x, y);
    window_state(w)->drag_drop = FALSE;
    GdkAtom target = gtk_drag_dest_find_target(w, ctx, NULL);
    if (target == GDK_NONE) return FALSE;
    gtk_drag_get_data(w, ctx, target, time);
    gdk_drag_status(ctx, GDK_ACTION_COPY, time);
    return TRUE;
}

static gboolean on_drag_drop(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, guint time, gpointer d) {
    (void)d;
    drag_at(w, x, y);
    window_state(w)->drag_drop = TRUE;
    GdkAtom target = gtk_drag_dest_find_target(w, ctx, NULL);
    if (target == GDK_NONE) return FALSE;
    gtk_drag_get_data(w, ctx, target, time);
    return TRUE;
}

static void on_drag_data(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, GtkSelectionData* data,
                         guint info, guint time, gpointer d) {
    (void)x; (void)y; (void)info; (void)d;
    NgLinuxWindow* state = window_state(w);
    gboolean dropping = state->drag_drop;
    char* paths = drop_paths(data);
    if (paths) {
        ng_invoke_file_drop((void*)w, dropping ? NG_DROP_DROP : NG_DROP_HOVER,
                            state->drag_x, state->drag_y, paths);
        g_free(paths);
    }
    if (dropping) gtk_drag_finish(ctx, paths != NULL, FALSE, time);
}

static void on_drag_leave(GtkWidget* w, GdkDragContext* ctx, guint time, gpointer d) {
    (void)ctx; (void)time; (void)d;
    ng_invoke_file_drop((void*)w, NG_DROP_LEAVE, 0, 0, "");
}
#endif

static void on_realize_uncompressed(GtkWidget* widget, gpointer data) {
    (void)data;
    GdkWindow* window = gtk_widget_get_window(widget);
    if (window) gdk_window_set_event_compression(window, FALSE);
}

/* A window asked bigger than the monitor's work area would hide its own edges. */
static void ng_linux_fit_work_area(int* width, int* height) {
    GdkDisplay* display = gdk_display_get_default();
    if (!display) return;
    GdkMonitor* monitor = gdk_display_get_primary_monitor(display);
    if (!monitor && gdk_display_get_n_monitors(display) > 0) monitor = gdk_display_get_monitor(display, 0);
    if (!monitor) return;
    GdkRectangle work;
    gdk_monitor_get_workarea(monitor, &work);
    if (work.width > 0 && *width > work.width) *width = work.width;
    if (work.height > 0 && *height > work.height) *height = work.height;
}

static unsigned int ng_linux_modifiers(GdkModifierType state) {
    unsigned int mods = 0;
    if (state & GDK_SHIFT_MASK) mods |= NG_MOD_SHIFT;
    if (state & GDK_CONTROL_MASK) mods |= NG_MOD_CTRL;
    if (state & GDK_MOD1_MASK) mods |= NG_MOD_ALT;
    if (state & GDK_SUPER_MASK || state & GDK_META_MASK) mods |= NG_MOD_META;
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

/* Preedit and the input method's key filter. Without ime, keys go straight through. */
#ifdef NG_INPUT_IME
static int ng_linux_composing(GtkWidget* widget) {
    return ng_linux_ime_composing(widget);
}

static int ng_linux_im_filter(GtkWidget* widget, GdkEventKey* event) {
    return ng_linux_ime_filter(widget, event);
}
#else
static int ng_linux_composing(GtkWidget* widget) {
    (void)widget;
    return 0;
}

static int ng_linux_im_filter(GtkWidget* widget, GdkEventKey* event) {
    (void)widget;
    (void)event;
    return 0;
}
#endif

#ifdef NG_INPUT_TEXT
/* Whether the input method makes a key's text. Without ime, the key's own text is used. */
static int ng_linux_text_from_im(GtkWidget* widget) {
#ifdef NG_INPUT_IME
    return ng_linux_ime_active(widget);
#else
    (void)widget;
    return 0;
#endif
}
#endif

static gboolean on_key_press(GtkWidget* widget, GdkEventKey* event, gpointer data) {
    (void)data;
    unsigned int mods = ng_linux_modifiers(event->state);
    unsigned int keycode = ng_linux_keycode_from_keyval(event->keyval);

    if (!ng_linux_composing(widget)) {
#ifdef NG_FEATURE_MENU
        if (ng_linux_handle_menu_shortcut((void*)widget, keycode, mods)) {
            window_state(widget)->menu_took_key = event->keyval;
            return TRUE;
        }
#endif
        ng_invoke_key_event((void*)widget, keycode, 1, mods);
        if (ng_shortcuts_match((void*)widget, keycode, mods)) return TRUE;
    }

    if (ng_linux_im_filter(widget, event)) return TRUE;
#ifdef NG_INPUT_TEXT
    if (!ng_linux_text_from_im(widget) && event->string && event->string[0] != '\0') {
        ng_invoke_text_input((void*)widget, event->string);
    }
#endif
    return FALSE;
}

static gboolean on_key_release(GtkWidget* widget, GdkEventKey* event, gpointer data) {
    (void)data;
#ifdef NG_FEATURE_MENU
    NgLinuxWindow* w = window_state(widget);
    if (w->menu_took_key != 0 && w->menu_took_key == event->keyval) {
        w->menu_took_key = 0;
        return TRUE;
    }
#endif
    unsigned int mods = ng_linux_modifiers(event->state);
    unsigned int keycode = ng_linux_keycode_from_keyval(event->keyval);
    int composing = ng_linux_composing(widget);
    if (ng_linux_im_filter(widget, event)) return TRUE;
    if (!composing) ng_invoke_key_event((void*)widget, keycode, 0, mods);
    return FALSE;
}

/* Whether this event is one the window already handled. Identical type, time, button and root
   position means the same event delivered twice. */
static gboolean seen_before(GtkWidget* widget, GdkEvent* event) {
    NgLinuxWindow* w = window_state(widget);
    double x = 0.0;
    double y = 0.0;
    gdk_event_get_root_coords(event, &x, &y);
    guint button = 0;
    if (event->type == GDK_BUTTON_PRESS || event->type == GDK_BUTTON_RELEASE) button = event->button.button;
    NgLinuxLastEvent now = {TRUE, event->type, gdk_event_get_time(event), button, x, y};
    NgLinuxLastEvent* last = &w->last;
    gboolean same = last->valid && last->type == now.type && last->time == now.time &&
        last->button == now.button && last->x == now.x && last->y == now.y;
    *last = now;
    return same;
}

static NgLinuxPointer* ng_linux_pointer(GtkWidget* window) {
    return &window_state(window)->pointer;
}

static GtkWidget* ng_linux_window_content(GtkWidget* window) {
    GtkWidget* vbox = main_vbox_of(window);
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

void ng_linux_content_origin(GtkWidget* window, int* x, int* y) {
    GtkWidget* content = ng_linux_window_content(window);
    *x = 0;
    *y = 0;
    if (content && !gtk_widget_translate_coordinates(content, window, 0, 0, x, y)) {
        *x = 0;
        *y = 0;
    }
}

static void ng_linux_content_point(GtkWidget* window, GdkWindow* from, double* x, double* y) {
    GdkWindow* top = gtk_widget_get_window(window);
    while (from && from != top) {
        gdk_window_coords_to_parent(from, *x, *y, x, y);
        from = gdk_window_get_parent(from);
    }
    int ox = 0;
    int oy = 0;
    ng_linux_content_origin(window, &ox, &oy);
    *x -= ox;
    *y -= oy;
}

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

#ifdef NG_INPUT_PEN
enum { PEN_HOVER = 0, PEN_DOWN = 1, PEN_MOVE = 2, PEN_UP = 3 };
enum { PEN_ERASER = 1, PEN_BARREL = 2 };

static double pen_age(GdkEvent* event) {
    guint32 stamp = gdk_event_get_time(event);
    if (stamp == GDK_CURRENT_TIME) return 0.0;
    guint32 now = (guint32)(g_get_monotonic_time() / 1000);
    return (double)(guint32)(now - stamp) / 1000.0;
}

static double pen_axis(GdkEvent* event, GdkAxisUse axis, double fallback) {
    double value = 0.0;
    return gdk_event_get_axis(event, axis, &value) ? value : fallback;
}

static void report_pen(GtkWidget* widget, GdkEvent* event, double x, double y, guint state, unsigned int phase) {
    GdkDevice* device = gdk_event_get_source_device(event);
    if (!device) return;
    GdkInputSource source = gdk_device_get_source(device);
    if (source != GDK_SOURCE_PEN && source != GDK_SOURCE_ERASER) return;
    double pressure = pen_axis(event, GDK_AXIS_PRESSURE, phase == PEN_HOVER ? 0.0 : 1.0);
    float tilt_x = (float)(pen_axis(event, GDK_AXIS_XTILT, 0.0) * 90.0);
    float tilt_y = (float)(pen_axis(event, GDK_AXIS_YTILT, 0.0) * 90.0);
    float twist = (float)(pen_axis(event, GDK_AXIS_ROTATION, 0.0) * 360.0);
    unsigned int flags = 0;
    if (source == GDK_SOURCE_ERASER) flags |= PEN_ERASER;
    if (state & (GDK_BUTTON2_MASK | GDK_BUTTON3_MASK)) flags |= PEN_BARREL;
    ng_invoke_pen((void*)widget, x, y, (float)pressure, tilt_x, tilt_y, twist, phase, flags,
                  ng_linux_modifiers(state), pen_age(event));
}
#endif

/* NG_POINTER_PEN or NG_POINTER_TOUCH when the device behind `event` is one, else 0. */
static unsigned int ng_linux_pointer_bits(GdkEvent* event) {
    GdkDevice* device = gdk_event_get_source_device(event);
    if (!device) return 0;
    switch (gdk_device_get_source(device)) {
#ifdef NG_INPUT_PEN
        case GDK_SOURCE_PEN:
        case GDK_SOURCE_ERASER:
            return NG_POINTER_PEN;
#endif
#ifdef NG_INPUT_TOUCH
        case GDK_SOURCE_TOUCHSCREEN:
            return NG_POINTER_TOUCH;
#endif
        default:
            return 0;
    }
}

static gboolean on_button_press(GtkWidget* widget, GdkEventButton* event, gpointer data) {
    (void)data;
    if (event->type != GDK_BUTTON_PRESS) return FALSE;
    if (seen_before(widget, (GdkEvent*)event)) return FALSE;
    unsigned int mods = ng_linux_modifiers(event->state) | ng_linux_pointer_bits((GdkEvent*)event);
    int button = ng_linux_mouse_button_from_event(event->button);
    int click_count = ng_linux_click_count(widget, event);
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    if (button < 8) ng_linux_pointer(widget)->held |= 1u << button;
#ifdef NG_INPUT_IME
    ng_linux_ime_reset(widget);
#endif
    ng_invoke_mouse_button((void*)widget, button, 1, mods, x, y, click_count);
#ifdef NG_INPUT_PEN
    if (event->button == 1) report_pen(widget, (GdkEvent*)event, x, y, event->state, PEN_DOWN);
#endif
    return FALSE;
}

static gboolean on_button_release(GtkWidget* widget, GdkEventButton* event, gpointer data) {
    (void)data;
    if (seen_before(widget, (GdkEvent*)event)) return FALSE;
    unsigned int mods = ng_linux_modifiers(event->state) | ng_linux_pointer_bits((GdkEvent*)event);
    int button = ng_linux_mouse_button_from_event(event->button);
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    if (button < 8) ng_linux_pointer(widget)->held &= ~(1u << button);
    ng_invoke_mouse_button((void*)widget, button, 0, mods, x, y, 1);
#ifdef NG_INPUT_PEN
    if (event->button == 1) report_pen(widget, (GdkEvent*)event, x, y, event->state, PEN_UP);
#endif
    return FALSE;
}

static gboolean on_motion_notify(GtkWidget* widget, GdkEventMotion* event, gpointer data) {
    (void)data;
    if (seen_before(widget, (GdkEvent*)event)) return FALSE;
    double x = event->x;
    double y = event->y;
    ng_linux_content_point(widget, event->window, &x, &y);
    unsigned int buttons = 0;
    if (event->state & GDK_BUTTON1_MASK) buttons |= 1u << 0;
    if (event->state & GDK_BUTTON3_MASK) buttons |= 1u << 1;
    if (event->state & GDK_BUTTON2_MASK) buttons |= 1u << 2;
    buttons |= ng_linux_pointer(widget)->held & ~7u;
    ng_invoke_mouse_move((void*)widget, x, y, buttons,
                         ng_linux_modifiers(event->state) | ng_linux_pointer_bits((GdkEvent*)event));
#ifdef NG_INPUT_PEN
    report_pen(widget, (GdkEvent*)event, x, y, event->state,
               (event->state & GDK_BUTTON1_MASK) ? PEN_MOVE : PEN_HOVER);
#endif

#ifdef NG_INPUT_RAW_MOUSE
    NgLinuxWindow* w = window_state(widget);
    if (w->grab_mode == 2) {
        if (w->mouse_known) {
            ng_invoke_raw_mouse_motion((void*)widget, event->x - w->mouse_x, event->y - w->mouse_y);
        }
        w->mouse_known = TRUE;
        w->mouse_x = event->x;
        w->mouse_y = event->y;
    }
#endif
    return FALSE;
}

static gboolean on_scroll(GtkWidget* widget, GdkEventScroll* event, gpointer data) {
    (void)data;
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

    ng_invoke_mouse_wheel((void*)widget, dx, -dy, mods);
    return FALSE;
}

#ifdef NG_INPUT_GESTURES
static void on_zoom_changed(GtkGestureZoom* gesture, gdouble scale, gpointer data) {
    (void)scale;
    gdouble delta = gtk_gesture_zoom_get_scale_delta(gesture);
    gdouble x = 0.0, y = 0.0;
    gtk_gesture_get_bounding_box_center(GTK_GESTURE(gesture), &x, &y);
    if (delta != 1.0) ng_invoke_gesture(data, NG_GESTURE_MAGNIFY, delta, x, y);
}

static void on_angle_changed(GtkGestureRotate* gesture, gdouble angle, gdouble angle_delta, gpointer data) {
    (void)angle;
    gdouble x = 0.0, y = 0.0;
    gtk_gesture_get_bounding_box_center(GTK_GESTURE(gesture), &x, &y);
    if (angle_delta != 0.0) ng_invoke_gesture(data, NG_GESTURE_ROTATE, angle_delta, x, y);
}

/* The gestures live as long as the window, which keeps them referenced. */
static void ng_linux_connect_gestures(GtkWidget* window) {
    NgLinuxWindow* w = window_state(window);
    w->zoom = gtk_gesture_zoom_new(window);
    g_signal_connect(w->zoom, "scale-changed", G_CALLBACK(on_zoom_changed), window);
    w->rotate = gtk_gesture_rotate_new(window);
    g_signal_connect(w->rotate, "angle-changed", G_CALLBACK(on_angle_changed), window);
}
#endif

static gboolean on_focus_in(GtkWidget* widget, GdkEventFocus* event, gpointer data) {
    (void)event; (void)data;
#ifdef NG_INPUT_IME
    ng_linux_ime_focus(widget, 1);
#endif
    gtk_window_set_urgency_hint(GTK_WINDOW(widget), FALSE);
    ng_invoke_focus_changed((void*)widget, 1);
    return FALSE;
}

static gboolean on_focus_out(GtkWidget* widget, GdkEventFocus* event, gpointer data) {
    (void)event; (void)data;
#ifdef NG_INPUT_IME
    ng_linux_ime_focus(widget, 0);
#endif
    ng_invoke_focus_changed((void*)widget, 0);
    return FALSE;
}

static gboolean ng_linux_crossing_is_inside(GtkWidget* widget, GdkEventCrossing* event) {
    return ng_linux_pointer(widget)->held != 0 ||
        event->detail == GDK_NOTIFY_INFERIOR ||
        event->window != gtk_widget_get_window(widget);
}

static gboolean on_enter(GtkWidget* widget, GdkEventCrossing* event, gpointer data) {
    (void)data;
    if (ng_linux_crossing_is_inside(widget, event)) return FALSE;
    ng_invoke_cursor_entered((void*)widget, 1);
    return FALSE;
}

static gboolean on_leave(GtkWidget* widget, GdkEventCrossing* event, gpointer data) {
    (void)data;
    if (ng_linux_crossing_is_inside(widget, event)) return FALSE;
    ng_invoke_cursor_entered((void*)widget, 0);
    return FALSE;
}

/* One configure handler: the scale, the restored rect and the lifecycle moves. */
static gboolean on_configure_event(GtkWidget* widget, GdkEventConfigure* event, gpointer data) {
    (void)data;
    NgLinuxWindow* w = window_state(widget);
    GdkWindow* gdk = gtk_widget_get_window(widget);
    if (gdk && w->on_scale) w->on_scale((void*)widget, (float)gdk_window_get_scale_factor(gdk));
    if (!(gdk && (gdk_window_get_state(gdk) & NG_LINUX_UNSIZED_STATES))) {
        gtk_window_get_position(GTK_WINDOW(widget), &w->restored[0], &w->restored[1]);
        gtk_window_get_size(GTK_WINDOW(widget), &w->restored[2], &w->restored[3]);
        w->has_restored = TRUE;
    }
    if (!w->lifecycle) return FALSE;
    if (w->x != event->x || w->y != event->y) {
        w->x = event->x;
        w->y = event->y;
        ng_invoke_lifecycle_callback((void*)widget, 11);
    }
    if (w->width != event->width || w->height != event->height) {
        w->width = event->width;
        w->height = event->height;
        ng_invoke_lifecycle_callback((void*)widget, 12);
    }
    return FALSE;
}

NGHandle ng_linux_create_window(const char* title, int width, int height) {
    if (!title) return NULL;

    GtkWidget* window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), title);
    ng_linux_fit_work_area(&width, &height);
    gtk_window_set_default_size(GTK_WINDOW(window), width, height);

    gtk_widget_add_events(
        window,
        GDK_KEY_PRESS_MASK | GDK_KEY_RELEASE_MASK | GDK_BUTTON_PRESS_MASK |
            GDK_BUTTON_RELEASE_MASK | GDK_POINTER_MOTION_MASK | GDK_SCROLL_MASK |
            GDK_ENTER_NOTIFY_MASK | GDK_LEAVE_NOTIFY_MASK | GDK_FOCUS_CHANGE_MASK);

    window_state(window);
#ifdef NG_INPUT_IME
    ng_linux_ime_attach(window);
#endif
    g_signal_connect(window, "realize", G_CALLBACK(on_realize_uncompressed), NULL);
    g_signal_connect(window, "key-press-event", G_CALLBACK(on_key_press), NULL);
    g_signal_connect(window, "key-release-event", G_CALLBACK(on_key_release), NULL);
    g_signal_connect(window, "button-press-event", G_CALLBACK(on_button_press), NULL);
    g_signal_connect(window, "button-release-event", G_CALLBACK(on_button_release), NULL);
    g_signal_connect(window, "motion-notify-event", G_CALLBACK(on_motion_notify), NULL);
    g_signal_connect(window, "scroll-event", G_CALLBACK(on_scroll), NULL);
    g_signal_connect(window, "focus-in-event", G_CALLBACK(on_focus_in), NULL);
    g_signal_connect(window, "focus-out-event", G_CALLBACK(on_focus_out), NULL);
    g_signal_connect(window, "enter-notify-event", G_CALLBACK(on_enter), NULL);
    g_signal_connect(window, "leave-notify-event", G_CALLBACK(on_leave), NULL);
#ifdef NG_INPUT_DROP_FILES
    gtk_drag_dest_set(window, 0, NULL, 0, GDK_ACTION_COPY);
    gtk_drag_dest_add_uri_targets(window);
    g_signal_connect(window, "drag-motion", G_CALLBACK(on_drag_motion), NULL);
    g_signal_connect(window, "drag-drop", G_CALLBACK(on_drag_drop), NULL);
    g_signal_connect(window, "drag-data-received", G_CALLBACK(on_drag_data), NULL);
    g_signal_connect(window, "drag-leave", G_CALLBACK(on_drag_leave), NULL);
#endif

    g_signal_connect(window, "destroy", G_CALLBACK(on_window_destroy), NULL);
    g_signal_connect(window, "delete-event", G_CALLBACK(on_delete_event), NULL);
    g_signal_connect(window, "window-state-event", G_CALLBACK(on_window_state_event), NULL);
    g_signal_connect(window, "configure-event", G_CALLBACK(on_configure_event), NULL);
#ifdef NG_INPUT_GESTURES
    ng_linux_connect_gestures(window);
#endif

    GtkWidget* main_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), main_vbox);
    window_state(window)->main_vbox = main_vbox;

    gtk_widget_show_all(window);
    return (NGHandle)window;
}

NGHandle ng_linux_create_window_with_type(const char* title, int width, int height, int window_type) {
    NGHandle handle = ng_linux_create_window(title, width, height);
    if (!handle) return NULL;

    GdkWindowTypeHint hint = GDK_WINDOW_TYPE_HINT_NORMAL;
    switch (window_type) {
        case 1:
            hint = GDK_WINDOW_TYPE_HINT_POPUP_MENU;
            break;
        case 2:
            hint = GDK_WINDOW_TYPE_HINT_TOOLBAR;
            break;
        case 3:
            hint = GDK_WINDOW_TYPE_HINT_UTILITY;
            break;
        case 4:
        case 5:
            hint = GDK_WINDOW_TYPE_HINT_DIALOG;
            break;
        default:
            break;
    }
    gtk_window_set_type_hint(GTK_WINDOW(handle), hint);
    return handle;
}

float ng_linux_get_scale_factor(NGHandle window) {
    if (!GTK_IS_WIDGET(window)) return 1.0f;
    GdkWindow* gdk = gtk_widget_get_window((GtkWidget*)window);
    return gdk ? (float)gdk_window_get_scale_factor(gdk) : 1.0f;
}

void ng_linux_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!GTK_IS_WINDOW(window)) return;
    window_state((GtkWidget*)window)->on_scale = callback;
}

void ng_linux_destroy_window(NGHandle handle) {
    if (!GTK_IS_WINDOW(handle)) return;
    gtk_widget_destroy((GtkWidget*)handle);
}

/* The caller owns an element with no parent: still floating, or the ref detach kept. Destroy alone
   leaves that ref, so drop it too. A parented one goes with its parent's ref. */
void ng_linux_destroy_element(NGHandle element) {
    if (!GTK_IS_WIDGET(element)) return;
    GtkWidget* widget = (GtkWidget*)element;
    gboolean ours = !gtk_widget_get_parent(widget) && !GTK_IS_WINDOW(widget);
    if (ours && g_object_is_floating(widget)) g_object_ref_sink(widget);
    gtk_widget_destroy(widget);
    if (ours) g_object_unref(widget);
}

void ng_linux_element_retain(NGHandle element) {
    if (GTK_IS_WIDGET(element)) g_object_ref(element);
}

void ng_linux_element_release(NGHandle element) {
    if (GTK_IS_WIDGET(element)) g_object_unref(element);
}

/* Takes the element out of its parent and keeps one reference for the caller. */
int ng_linux_detach_element(NGHandle element) {
    if (!GTK_IS_WIDGET(element)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* widget = (GtkWidget*)element;
    GtkWidget* parent = gtk_widget_get_parent(widget);
    if (!parent) return NG_SUCCESS;
    if (!GTK_IS_CONTAINER(parent)) return NG_ERROR_PLATFORM_SPECIFIC;
    g_object_ref(widget);
    ng_linux_unparent(widget);
    return NG_SUCCESS;
}

void ng_linux_window_focus(NGHandle window) {
    if (GTK_IS_WINDOW(window)) gtk_window_present(GTK_WINDOW(window));
}

void ng_linux_window_show(NGHandle window) {
    if (GTK_IS_WIDGET(window)) gtk_widget_show(GTK_WIDGET(window));
}

void ng_linux_window_hide(NGHandle window) {
    if (GTK_IS_WIDGET(window)) gtk_widget_hide(GTK_WIDGET(window));
}

void ng_linux_window_set_close_guard(NGHandle window, int guarded) {
    if (!GTK_IS_WINDOW(window)) return;
    window_state((GtkWidget*)window)->close_guard = guarded ? TRUE : FALSE;
}

int ng_linux_window_is_visible(NGHandle window) {
    if (!GTK_IS_WIDGET(window)) return 0;
    return gtk_widget_get_visible((GtkWidget*)window) ? 1 : 0;
}

/* GTK says only whether the window is shown and not minimized; other windows over it are not
 * known, so a shown window reads as visible. */
int ng_linux_window_occlusion(NGHandle window) {
    if (!GTK_IS_WIDGET(window) || !gtk_widget_get_visible((GtkWidget*)window)) return 2;
    GdkWindow* gdk = gtk_widget_get_window((GtkWidget*)window);
    if (gdk && (gdk_window_get_state(gdk) & GDK_WINDOW_STATE_ICONIFIED)) return 2;
    return 0;
}

int ng_linux_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!GTK_IS_WINDOW(window_handle) || !GTK_IS_WIDGET(content_handle)) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* window = (GtkWidget*)window_handle;
    GtkWidget* content = (GtkWidget*)content_handle;
    GtkWidget* vbox = main_vbox_of(window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;

    /* The content may be one of the children removed here, so keep it alive across the removal. */
    g_object_ref(content);
    GList* children = gtk_container_get_children(GTK_CONTAINER(vbox));
    for (GList* item = children; item != NULL; item = item->next) {
        GtkWidget* old = GTK_WIDGET(item->data);
        if (GTK_IS_MENU_BAR(old)) continue;
        /* The old content stays alive for its owner, which detaches and destroys it. */
        if (old != content) g_object_ref(old);
        gtk_container_remove(GTK_CONTAINER(vbox), old);
    }
    g_list_free(children);

    ng_linux_unparent(content);
    gtk_box_pack_end(GTK_BOX(vbox), content, TRUE, TRUE, 0);
    g_object_unref(content);
    gtk_widget_show_all(window);
    return NG_SUCCESS;
}

void ng_linux_window_set_lifecycle_callback(NGHandle window) {
    if (!GTK_IS_WINDOW(window)) return;
    GtkWidget* widget = (GtkWidget*)window;
    NgLinuxWindow* w = window_state(widget);
    if (w->lifecycle) return;
    w->lifecycle = TRUE;
    gtk_window_get_position(GTK_WINDOW(widget), &w->x, &w->y);
    gtk_window_get_size(GTK_WINDOW(widget), &w->width, &w->height);
}

NGHandle ng_linux_window_get_content_view(NGHandle window) {
    return (NGHandle)main_vbox_of((GtkWidget*)window);
}

void ng_linux_window_set_title(NGHandle window, const char* title) {
    if (!GTK_IS_WINDOW(window) || !title) return;
    gtk_window_set_title(GTK_WINDOW(window), title);
}

int ng_linux_window_set_icon_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height) {
    if (!GTK_IS_WINDOW(window)) return NG_ERROR_INVALID_HANDLE;
    if (!rgba || width == 0 || height == 0 || width > INT_MAX / 4 || height > INT_MAX) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    GdkPixbuf* pixbuf = gdk_pixbuf_new(GDK_COLORSPACE_RGB, TRUE, 8, (int)width, (int)height);
    if (!pixbuf) return NG_ERROR_PLATFORM_SPECIFIC;

    unsigned char* pixels = gdk_pixbuf_get_pixels(pixbuf);
    int row_stride = gdk_pixbuf_get_rowstride(pixbuf);
    size_t source_stride = (size_t)width * 4;
    for (unsigned int row = 0; row < height; ++row) {
        memcpy(pixels + (size_t)row * (size_t)row_stride, rgba + (size_t)row * source_stride, source_stride);
    }
    gtk_window_set_icon(GTK_WINDOW(window), pixbuf);
    g_object_unref(pixbuf);
    return NG_SUCCESS;
}

void ng_linux_window_set_size(NGHandle window, int width, int height) {
    if (!GTK_IS_WINDOW(window)) return;
    gtk_window_resize(GTK_WINDOW(window), width, height);
}

int ng_linux_window_set_min_size(NGHandle window, int width, int height) {
    if (!GTK_IS_WINDOW(window)) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    GtkWidget* vbox = main_vbox_of((GtkWidget*)window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;
    gtk_widget_set_size_request(vbox, width ? width : -1, height ? height : -1);
    int current_w = 0, current_h = 0;
    gtk_window_get_size(GTK_WINDOW(window), &current_w, &current_h);
    if (current_w < width || current_h < height) {
        gtk_window_resize(GTK_WINDOW(window), current_w > width ? current_w : width,
                          current_h > height ? current_h : height);
    }
    return NG_SUCCESS;
}

void ng_linux_window_get_size(NGHandle window, int* width, int* height) {
    if (!GTK_IS_WINDOW(window) || !width || !height) return;
    gtk_window_get_size(GTK_WINDOW(window), width, height);
}

void ng_linux_window_set_position(NGHandle window, int x, int y) {
    if (!GTK_IS_WINDOW(window)) return;
    gtk_window_move(GTK_WINDOW(window), x, y);
}

void ng_linux_window_get_position(NGHandle window, int* x, int* y) {
    if (!GTK_IS_WINDOW(window) || !x || !y) return;
    gtk_window_get_position(GTK_WINDOW(window), x, y);
}

int ng_linux_window_refresh_rate(NGHandle window) {
    if (!GTK_IS_WIDGET(window)) return 0;
    GdkWindow* gdk = gtk_widget_get_window((GtkWidget*)window);
    GdkDisplay* display = gtk_widget_get_display((GtkWidget*)window);
    GdkMonitor* monitor = gdk && display ? gdk_display_get_monitor_at_window(display, gdk) : NULL;
    return monitor ? gdk_monitor_get_refresh_rate(monitor) : 0;
}

int ng_linux_window_request_attention(NGHandle window, int critical) {
    (void)critical;
    if (!GTK_IS_WINDOW(window)) return NG_ERROR_INVALID_HANDLE;
    if (!gtk_window_is_active(GTK_WINDOW(window))) gtk_window_set_urgency_hint(GTK_WINDOW(window), TRUE);
    return NG_SUCCESS;
}

int ng_linux_window_get_placement(NGHandle window, int* rect) {
    if (!GTK_IS_WINDOW(window) || !rect) return NG_ERROR_INVALID_HANDLE;
    GtkWindow* gtk = GTK_WINDOW(window);
    NgLinuxWindow* w = window_state((GtkWidget*)window);
    if (w->has_restored) {
        memcpy(rect, w->restored, sizeof w->restored);
    } else {
        gtk_window_get_position(gtk, &rect[0], &rect[1]);
        gtk_window_get_size(gtk, &rect[2], &rect[3]);
    }
    GdkWindow* gdk = gtk_widget_get_window(GTK_WIDGET(window));
    GdkWindowState state = gdk ? gdk_window_get_state(gdk) : 0;
    return (gtk_window_is_maximized(gtk) ? 1 : 0) | ((state & GDK_WINDOW_STATE_ICONIFIED) ? 2 : 0);
}

int ng_linux_window_set_placement(NGHandle window, const int* rect, int maximized) {
    if (!GTK_IS_WINDOW(window) || !rect) return NG_ERROR_INVALID_HANDLE;
    if (rect[2] <= 0 || rect[3] <= 0) return NG_ERROR_INVALID_PARAMETER;
    GtkWindow* gtk = GTK_WINDOW(window);
    int x = rect[0], y = rect[1];
    /* off every monitor now, as after unplugging one: put it on the primary one */
    GdkDisplay* display = gtk_widget_get_display(GTK_WIDGET(window));
    GdkRectangle want = {x, y, rect[2], rect[3]};
    gboolean seen = FALSE;
    for (int i = 0; display && i < gdk_display_get_n_monitors(display); i++) {
        GdkRectangle area;
        gdk_monitor_get_workarea(gdk_display_get_monitor(display, i), &area);
        if (gdk_rectangle_intersect(&want, &area, NULL)) seen = TRUE;
    }
    GdkMonitor* primary = display ? gdk_display_get_primary_monitor(display) : NULL;
    if (!primary && display) primary = gdk_display_get_monitor(display, 0);
    if (!seen && primary) {
        GdkRectangle area;
        gdk_monitor_get_workarea(primary, &area);
        x = area.x;
        y = area.y;
    }
    gtk_window_unmaximize(gtk);
    gtk_window_move(gtk, x, y);
    gtk_window_resize(gtk, rect[2], rect[3]);
    if (maximized) gtk_window_maximize(gtk);
    return NG_SUCCESS;
}

void ng_linux_window_request_close(NGHandle window) {
    if (GTK_IS_WINDOW(window)) gtk_window_close(GTK_WINDOW(window));
}

int ng_linux_window_is_focused(NGHandle window) {
    if (!GTK_IS_WINDOW(window)) return 0;
    return gtk_window_is_active(GTK_WINDOW(window)) ? 1 : 0;
}

int ng_linux_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection) {
    if (!GTK_IS_WIDGET(window) || !xcb_window || !xcb_connection) return 0;
    GdkWindow* gdk_window = gtk_widget_get_window((GtkWidget*)window);
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
    if (!GTK_IS_WIDGET(window) || !surface || !display) return 0;
    GdkWindow* gdk_window = gtk_widget_get_window((GtkWidget*)window);
    if (!gdk_window) return 0;
#ifdef GDK_WINDOWING_WAYLAND
    if (GDK_IS_WAYLAND_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display) return 0;
        *surface = gdk_wayland_window_get_wl_surface(gdk_window);
        *display = gdk_wayland_display_get_wl_display(gdk_display);
        if (*surface == NULL || *display == NULL) return 0;
        return 1;
    }
#endif
    return 0;
}

int ng_linux_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y) {
    if (!GTK_IS_WIDGET(window) || !rgba || !width || !height || width > INT_MAX / 4 || height > INT_MAX ||
        hot_x >= width || hot_y >= height) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    GdkWindow* native = gtk_widget_get_window((GtkWidget*)window);
    if (!native) return NG_ERROR_INVALID_HANDLE;
    cairo_surface_t* image = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, (int)width, (int)height);
    if (cairo_surface_status(image) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(image);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    unsigned char* data = cairo_image_surface_get_data(image);
    int stride = cairo_image_surface_get_stride(image);
    for (unsigned int y = 0; y < height; y++) {
        uint32_t* dst = (uint32_t*)(data + (size_t)y * stride);
        for (unsigned int x = 0; x < width; x++) {
            const unsigned char* p = rgba + ((size_t)y * width + x) * 4;
            dst[x] = ((uint32_t)p[3] << 24) | ((uint32_t)((p[0] * p[3] + 127) / 255) << 16) |
                     ((uint32_t)((p[1] * p[3] + 127) / 255) << 8) | (uint32_t)((p[2] * p[3] + 127) / 255);
        }
    }
    int scale = MAX(1, gdk_window_get_scale_factor(native));
    cairo_surface_mark_dirty(image);
    cairo_surface_set_device_scale(image, scale, scale);
    GdkCursor* cursor = gdk_cursor_new_from_surface(gdk_window_get_display(native), image,
                                                    (double)hot_x / scale, (double)hot_y / scale);
    cairo_surface_destroy(image);
    if (!cursor) return NG_ERROR_PLATFORM_SPECIFIC;
    gdk_window_set_cursor(native, cursor);
    g_object_unref(cursor);
    return NG_SUCCESS;
}

int ng_linux_window_set_cursor_icon(NGHandle window, int icon) {
    if (!GTK_IS_WIDGET(window)) return NG_ERROR_INVALID_HANDLE;
    GdkWindow* gdkWindow = gtk_widget_get_window((GtkWidget*)window);
    if (!gdkWindow) return NG_ERROR_INVALID_HANDLE;

    static const char* const names[] = {
        NULL, "pointer", "text", "crosshair", "move",
        "ew-resize", "ns-resize", "not-allowed", "wait",
        "nwse-resize", "nesw-resize",
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
    if (!GTK_IS_WIDGET(window)) return NG_ERROR_INVALID_HANDLE;
    GdkWindow* gdkWindow = gtk_widget_get_window((GtkWidget*)window);
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

/* A grab that fails leaves the window as it was. */
int ng_linux_window_set_cursor_grab(NGHandle window, int mode) {
    if (!GTK_IS_WIDGET(window)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* widget = (GtkWidget*)window;
    GdkWindow* gdkWindow = gtk_widget_get_window(widget);
    if (!gdkWindow) return NG_ERROR_INVALID_HANDLE;
    GdkDisplay* display = gdk_window_get_display(gdkWindow);
    if (!display) return NG_ERROR_PLATFORM_SPECIFIC;
    GdkSeat* seat = gdk_display_get_default_seat(display);
    if (!seat) return NG_ERROR_PLATFORM_SPECIFIC;

    NgLinuxWindow* w = window_state(widget);
    if (mode == 0) {
        gdk_seat_ungrab(seat);
    } else {
        GdkGrabStatus status = gdk_seat_grab(seat, gdkWindow, GDK_SEAT_CAPABILITY_POINTER, TRUE,
                                             NULL, NULL, NULL, NULL);
        if (status != GDK_GRAB_SUCCESS) return NG_ERROR_PLATFORM_SPECIFIC;
    }
    w->grab_mode = mode;
    w->mouse_known = FALSE;
    return NG_SUCCESS;
}

#ifdef NG_FEATURE_CLIPBOARD
char* ng_linux_get_clipboard_text(void) {
    GtkClipboard* cb = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    if (!cb) return NULL;
    return gtk_clipboard_wait_for_text(cb);
}

void ng_linux_free_clipboard_text(char* text) {
    g_free(text);
}

int ng_linux_set_clipboard_text(const char* text) {
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    GtkClipboard* cb = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    if (!cb) return NG_ERROR_PLATFORM_SPECIFIC;
    gtk_clipboard_set_text(cb, text, -1);
    return NG_SUCCESS;
}
#endif
