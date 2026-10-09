/* Input method support through GtkIMContext: committed text goes to the
 * application as text input, composed text as preedit, and the candidate
 * window follows the caret area the application reports. */

#include "ime.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"

#define AUREA_IM_KEY "aurea-im"
#define AUREA_IM_COMPOSING_KEY "aurea-im-composing"

static GtkIMContext* ng_linux_im(GtkWidget* window) {
    return window ? (GtkIMContext*)g_object_get_data(G_OBJECT(window), AUREA_IM_KEY) : NULL;
}

static void on_im_commit(GtkIMContext* context, const gchar* text, gpointer window) {
    (void)context;
    if (text && text[0] != '\0') ng_invoke_text_input(window, text);
}

static void on_im_preedit_changed(GtkIMContext* context, gpointer window) {
    gchar* text = NULL;
    gint cursor = 0;
    gtk_im_context_get_preedit_string(context, &text, NULL, &cursor);
    const gchar* shown = text ? text : "";

    /* GTK counts the caret in characters; Aurea in bytes. */
    glong length = g_utf8_strlen(shown, -1);
    int offset = -1;
    if (cursor >= 0 && cursor <= length) {
        offset = (int)(g_utf8_offset_to_pointer(shown, cursor) - shown);
    }

    g_object_set_data(G_OBJECT(window), AUREA_IM_COMPOSING_KEY, GINT_TO_POINTER(shown[0] != '\0'));
    ng_invoke_ime_preedit(window, shown, offset);
    g_free(text);
}

static void on_im_preedit_end(GtkIMContext* context, gpointer window) {
    (void)context;
    g_object_set_data(G_OBJECT(window), AUREA_IM_COMPOSING_KEY, GINT_TO_POINTER(0));
    ng_invoke_ime_preedit(window, "", -1);
}

static void on_window_realize(GtkWidget* window, gpointer user_data) {
    (void)user_data;
    GtkIMContext* im = ng_linux_im(window);
    if (im) gtk_im_context_set_client_window(im, gtk_widget_get_window(window));
}

static void on_window_unrealize(GtkWidget* window, gpointer user_data) {
    (void)user_data;
    GtkIMContext* im = ng_linux_im(window);
    if (im) gtk_im_context_set_client_window(im, NULL);
}

void ng_linux_ime_attach(GtkWidget* window) {
    GtkIMContext* im = gtk_im_multicontext_new();
    /* Owned by the window: dropped with it. */
    g_object_set_data_full(G_OBJECT(window), AUREA_IM_KEY, im, g_object_unref);
    g_signal_connect(im, "commit", G_CALLBACK(on_im_commit), window);
    g_signal_connect(im, "preedit-changed", G_CALLBACK(on_im_preedit_changed), window);
    g_signal_connect(im, "preedit-end", G_CALLBACK(on_im_preedit_end), window);
    g_signal_connect(window, "realize", G_CALLBACK(on_window_realize), NULL);
    g_signal_connect(window, "unrealize", G_CALLBACK(on_window_unrealize), NULL);
    if (gtk_widget_get_realized(window)) on_window_realize(window, NULL);
}

void ng_linux_ime_focus(GtkWidget* window, int focused) {
    GtkIMContext* im = ng_linux_im(window);
    if (!im) return;
    if (focused) {
        gtk_im_context_focus_in(im);
    } else {
        gtk_im_context_focus_out(im);
    }
}

int ng_linux_ime_composing(GtkWidget* window) {
    return window && GPOINTER_TO_INT(g_object_get_data(G_OBJECT(window), AUREA_IM_COMPOSING_KEY));
}

int ng_linux_ime_filter(GtkWidget* window, GdkEventKey* event) {
    GtkIMContext* im = ng_linux_im(window);
    return im ? gtk_im_context_filter_keypress(im, event) : 0;
}

int ng_linux_ime_available(GtkWidget* window) {
    return ng_linux_im(window) != NULL;
}

int ng_linux_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    GtkIMContext* im = ng_linux_im((GtkWidget*)window);
    if (!im) return NG_ERROR_INVALID_HANDLE;
    GdkRectangle area = {(int)x, (int)y, (int)width, (int)height};
    gtk_im_context_set_cursor_location(im, &area);
    return NG_SUCCESS;
}
