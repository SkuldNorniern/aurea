#include "drop.h"
#include "ime.h"
#include "window_state.h"
#include "common/rust_callbacks.h"

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
    NgLinuxWindow* state = ng_linux_window_state(w);
    state->drag_x = x - ox;
    state->drag_y = y - oy;
}

static gboolean on_drag_motion(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, guint time, gpointer d) {
    (void)d;
    drag_at(w, x, y);
    ng_linux_window_state(w)->drag_drop = FALSE;
    GdkAtom target = gtk_drag_dest_find_target(w, ctx, NULL);
    if (target == GDK_NONE) return FALSE;
    gtk_drag_get_data(w, ctx, target, time);
    gdk_drag_status(ctx, GDK_ACTION_COPY, time);
    return TRUE;
}

static gboolean on_drag_drop(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, guint time, gpointer d) {
    (void)d;
    drag_at(w, x, y);
    ng_linux_window_state(w)->drag_drop = TRUE;
    GdkAtom target = gtk_drag_dest_find_target(w, ctx, NULL);
    if (target == GDK_NONE) return FALSE;
    gtk_drag_get_data(w, ctx, target, time);
    return TRUE;
}

static void on_drag_data(GtkWidget* w, GdkDragContext* ctx, gint x, gint y, GtkSelectionData* data,
                         guint info, guint time, gpointer d) {
    (void)x; (void)y; (void)info; (void)d;
    NgLinuxWindow* state = ng_linux_window_state(w);
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

void ng_linux_accept_drops(GtkWidget* window) {
    gtk_drag_dest_set(window, 0, NULL, 0, GDK_ACTION_COPY);
    gtk_drag_dest_add_uri_targets(window);
    g_signal_connect(window, "drag-motion", G_CALLBACK(on_drag_motion), NULL);
    g_signal_connect(window, "drag-drop", G_CALLBACK(on_drag_drop), NULL);
    g_signal_connect(window, "drag-data-received", G_CALLBACK(on_drag_data), NULL);
    g_signal_connect(window, "drag-leave", G_CALLBACK(on_drag_leave), NULL);
}
