#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_progress_bar(void) {
    GtkWidget* progressBar = gtk_progress_bar_new();
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progressBar), 0.0);
    gtk_widget_show(progressBar);
    
    return (NGHandle)progressBar;
}

static void stop_pulse(GtkWidget* bar);

int ng_linux_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    if (!(value >= 0.0)) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    stop_pulse((GtkWidget*)progress_bar);
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progress_bar), value);
    gtk_widget_queue_draw((GtkWidget*)progress_bar);
    return NG_SUCCESS;
}

#define PULSE_KEY "aurea-pulse"

static gboolean pulse(gpointer bar) {
    gtk_progress_bar_pulse(GTK_PROGRESS_BAR(bar));
    return G_SOURCE_CONTINUE;
}

static void stop_pulse(GtkWidget* bar) {
    guint source = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(bar), PULSE_KEY));
    if (source) g_source_remove(source);
    g_object_set_data(G_OBJECT(bar), PULSE_KEY, NULL);
}

static void on_destroy(GtkWidget* bar, gpointer data) {
    (void)data;
    stop_pulse(bar);
}

int ng_linux_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar || !GTK_IS_PROGRESS_BAR(progress_bar)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* bar = (GtkWidget*)progress_bar;
    int running = g_object_get_data(G_OBJECT(bar), PULSE_KEY) != NULL;
    if (indeterminate && !running) {
        gtk_progress_bar_pulse(GTK_PROGRESS_BAR(bar));
        guint source = g_timeout_add(100, pulse, bar);
        g_object_set_data(G_OBJECT(bar), PULSE_KEY, GUINT_TO_POINTER(source));
        if (!g_object_get_data(G_OBJECT(bar), "aurea-pulse-hooked")) {
            g_signal_connect(bar, "destroy", G_CALLBACK(on_destroy), NULL);
            g_object_set_data(G_OBJECT(bar), "aurea-pulse-hooked", GINT_TO_POINTER(1));
        }
    } else if (!indeterminate && running) {
        stop_pulse(bar);
        gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(bar), 0.0);
    }
    return NG_SUCCESS;
}

int ng_linux_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    gtk_widget_set_sensitive((GtkWidget*)progress_bar, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_progress_bar_invalidate(NGHandle progress_bar) {
    if (!progress_bar) return;
    gtk_widget_queue_draw((GtkWidget*)progress_bar);
}

