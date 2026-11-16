#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

/* The pulse timer of an indeterminate bar. It exists only while the bar pulses, and removing it
   on free means a destroyed bar cannot be pulsed. */
typedef struct {
    guint source;
} PulseState;

#define PULSE_KEY "aurea-pulse"

static void pulse_free(gpointer data) {
    PulseState* pulse = data;
    if (pulse->source) g_source_remove(pulse->source);
    g_free(pulse);
}

static gboolean pulse(gpointer bar) {
    gtk_progress_bar_pulse(GTK_PROGRESS_BAR(bar));
    return G_SOURCE_CONTINUE;
}

static void stop_pulse(GtkWidget* bar) {
    g_object_set_data_full(G_OBJECT(bar), PULSE_KEY, NULL, NULL);
}

NGHandle ng_linux_create_progress_bar(void) {
    GtkWidget* bar = gtk_progress_bar_new();
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(bar), 0.0);
    gtk_widget_show(bar);
    return (NGHandle)bar;
}

int ng_linux_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar || !GTK_IS_PROGRESS_BAR(progress_bar)) return NG_ERROR_INVALID_HANDLE;
    if (!(value >= 0.0)) value = 0.0;
    if (value > 1.0) value = 1.0;
    stop_pulse((GtkWidget*)progress_bar);
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progress_bar), value);
    gtk_widget_queue_draw((GtkWidget*)progress_bar);
    return NG_SUCCESS;
}

int ng_linux_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar || !GTK_IS_PROGRESS_BAR(progress_bar)) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* bar = (GtkWidget*)progress_bar;
    gboolean running = g_object_get_data(G_OBJECT(bar), PULSE_KEY) != NULL;
    if (indeterminate && !running) {
        gtk_progress_bar_pulse(GTK_PROGRESS_BAR(bar));
        PulseState* pulse_state = g_new0(PulseState, 1);
        pulse_state->source = g_timeout_add(100, pulse, bar);
        g_object_set_data_full(G_OBJECT(bar), PULSE_KEY, pulse_state, pulse_free);
    } else if (!indeterminate && running) {
        stop_pulse(bar);
        gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(bar), 0.0);
    }
    return NG_SUCCESS;
}

int ng_linux_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    if (!progress_bar || !GTK_IS_WIDGET(progress_bar)) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_set_sensitive((GtkWidget*)progress_bar, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_progress_bar_invalidate(NGHandle progress_bar) {
    if (!progress_bar) return;
    gtk_widget_queue_draw((GtkWidget*)progress_bar);
}
