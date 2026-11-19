#include "ime.h"
#include <math.h>
#include "common/errors.h"
#include "common/rust_callbacks.h"

/* One per toplevel: its input method and whether it is mid-composition. Freed with the window. */
typedef struct {
    GtkWidget* window;
    GtkIMContext* context;
    gboolean composing;
    /* Kept out: keys skip the input method and type as they are. */
    gboolean blocked;
} NgIme;

#define AUREA_IME_KEY "aurea-ime"

static void ime_free(gpointer data) {
    NgIme* ime = data;
    g_object_unref(ime->context);
    g_free(ime);
}

static NgIme* ime_of(GtkWidget* window) {
    return GTK_IS_WINDOW(window) ? g_object_get_data(G_OBJECT(window), AUREA_IME_KEY) : NULL;
}

static void on_im_commit(GtkIMContext* context, const gchar* text, gpointer data) {
    (void)context;
    NgIme* ime = data;
    if (text && text[0] != '\0') ng_invoke_text_input(ime->window, text);
}

static void on_im_preedit_changed(GtkIMContext* context, gpointer data) {
    NgIme* ime = data;
    gchar* text = NULL;
    gint cursor = 0;
    gtk_im_context_get_preedit_string(context, &text, NULL, &cursor);
    const gchar* shown = text ? text : "";

    glong length = g_utf8_strlen(shown, -1);
    int offset = -1;
    if (cursor >= 0 && cursor <= length) {
        offset = (int)(g_utf8_offset_to_pointer(shown, cursor) - shown);
    }

    ime->composing = shown[0] != '\0';
    ng_invoke_ime_preedit(ime->window, shown, offset);
    g_free(text);
}

static void on_im_preedit_end(GtkIMContext* context, gpointer data) {
    (void)context;
    NgIme* ime = data;
    ime->composing = FALSE;
    ng_invoke_ime_preedit(ime->window, "", -1);
}

static void on_window_realize(GtkWidget* window, gpointer data) {
    NgIme* ime = data;
    gtk_im_context_set_client_window(ime->context, gtk_widget_get_window(window));
}

static void on_window_unrealize(GtkWidget* window, gpointer data) {
    (void)window;
    NgIme* ime = data;
    gtk_im_context_set_client_window(ime->context, NULL);
}

void ng_linux_ime_attach(GtkWidget* window) {
    NgIme* ime = g_new0(NgIme, 1);
    ime->window = window;
    ime->context = gtk_im_multicontext_new();
    g_object_set_data_full(G_OBJECT(window), AUREA_IME_KEY, ime, ime_free);
    g_signal_connect(ime->context, "commit", G_CALLBACK(on_im_commit), ime);
    g_signal_connect(ime->context, "preedit-changed", G_CALLBACK(on_im_preedit_changed), ime);
    g_signal_connect(ime->context, "preedit-end", G_CALLBACK(on_im_preedit_end), ime);
    g_signal_connect(window, "realize", G_CALLBACK(on_window_realize), ime);
    g_signal_connect(window, "unrealize", G_CALLBACK(on_window_unrealize), ime);
    if (gtk_widget_get_realized(window)) on_window_realize(window, ime);
}

void ng_linux_ime_focus(GtkWidget* window, int focused) {
    NgIme* ime = ime_of(window);
    if (!ime) return;
    if (focused && !ime->blocked) {
        gtk_im_context_focus_in(ime->context);
    } else {
        gtk_im_context_focus_out(ime->context);
    }
}

void ng_linux_ime_reset(GtkWidget* window) {
    NgIme* ime = ime_of(window);
    if (ime && ime->composing) gtk_im_context_reset(ime->context);
}

int ng_linux_ime_composing(GtkWidget* window) {
    NgIme* ime = ime_of(window);
    return ime && ime->composing;
}

int ng_linux_ime_active(GtkWidget* window) {
    NgIme* ime = ime_of(window);
    if (!ime || ime->blocked) return 0;
    GtkWidget* focus = gtk_window_get_focus(GTK_WINDOW(window));
    return focus == NULL || GTK_IS_DRAWING_AREA(focus);
}

int ng_linux_ime_filter(GtkWidget* window, GdkEventKey* event) {
    NgIme* ime = ime_of(window);
    if (!ime || !ng_linux_ime_active(window)) return 0;
    return gtk_im_context_filter_keypress(ime->context, event);
}

/* Keeps the input method out of `window`: a composition in flight is dropped, and no key goes
   through it until allowed again. */
int ng_linux_window_set_ime_allowed(NGHandle window, int allowed) {
    NgIme* ime = ime_of((GtkWidget*)window);
    if (!ime) return NG_ERROR_INVALID_HANDLE;
    gboolean blocked = !allowed;
    if (blocked == ime->blocked) return NG_SUCCESS;
    ime->blocked = blocked;
    if (blocked) {
        gtk_im_context_reset(ime->context);
        gtk_im_context_focus_out(ime->context);
    } else if (gtk_window_has_toplevel_focus(GTK_WINDOW(ime->window))) {
        gtk_im_context_focus_in(ime->context);
    }
    return NG_SUCCESS;
}

int ng_linux_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    NgIme* ime = ime_of((GtkWidget*)window);
    if (!ime) return NG_ERROR_INVALID_HANDLE;
    if (!(x > -1e7f && y > -1e7f && x < 1e7f && y < 1e7f)) return NG_ERROR_INVALID_PARAMETER;
    if (!(width >= 0.0f && width < 1e7f)) width = 0.0f;
    if (!(height >= 0.0f && height < 1e7f)) height = 0.0f;
    int ox = 0;
    int oy = 0;
    ng_linux_content_origin(ime->window, &ox, &oy);
    int left = (int)floor(x);
    int top = (int)floor(y);
    GdkRectangle area = {left + ox, top + oy, (int)ceil(x + width) - left, (int)ceil(y + height) - top};
    gtk_im_context_set_cursor_location(ime->context, &area);
    return NG_SUCCESS;
}
