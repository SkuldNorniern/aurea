#include "../elements.h"
#include "../../common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_progress_bar(void) {
    GtkWidget* progressBar = gtk_progress_bar_new();
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progressBar), 0.0);
    gtk_widget_show(progressBar);
    
    return (NGHandle)progressBar;
}

int ng_linux_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    if (value < 0.0) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progress_bar), value);
    gtk_widget_queue_draw((GtkWidget*)progress_bar);
    return NG_SUCCESS;
}

int ng_linux_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    GtkProgressBar* progressBar = GTK_PROGRESS_BAR(progress_bar);
    
    if (indeterminate) {
        gtk_progress_bar_pulse(progressBar);
    } else {
        gtk_progress_bar_set_fraction(progressBar, 0.0);
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

