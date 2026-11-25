#ifndef AUREA_LINUX_WINDOW_STATE_H
#define AUREA_LINUX_WINDOW_STATE_H

#include "common/platform_api.h"
#include <gtk/gtk.h>

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
    gboolean hide_on_close;
    /* The frame's place and size as last reported. */
    int x, y, width, height;
    /* The rect kept while not maximized, tiled, fullscreen or iconified. */
    int restored[4];
    gboolean has_restored;
    /* Set by set_placement until a configure ends it; an older configure must not record its own
       size as the restored one. */
    gboolean placement_pending;
    int pending_width, pending_height;
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

/* The state of a GTK window, made on first use. */
NgLinuxWindow* ng_linux_window_state(GtkWidget* window);
/* The NG_MOD_* bits for a GDK modifier state. */
unsigned int ng_linux_modifiers(GdkModifierType state);

#endif
