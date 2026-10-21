#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>
#include <math.h>

NGHandle ng_linux_create_slider(double min, double max) {
    if (!(min < max) || !(min > -HUGE_VAL) || !(max < HUGE_VAL)) return NULL;
    
    GtkWidget* scale = gtk_scale_new_with_range(
        GTK_ORIENTATION_HORIZONTAL,
        min, max, (max - min) / 100.0
    );
    gtk_range_set_round_digits(GTK_RANGE(scale), -1);
    gtk_scale_set_draw_value(GTK_SCALE(scale), FALSE);
    
    gtk_range_set_value(GTK_RANGE(scale), (min + max) / 2.0);
    gtk_widget_show(scale);
    
    return (NGHandle)scale;
}

int ng_linux_slider_set_value(NGHandle slider, double value) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    GtkRange* range = GTK_RANGE(slider);
    GtkAdjustment* adjustment = gtk_range_get_adjustment(range);
    double minVal = gtk_adjustment_get_lower(adjustment);
    double maxVal = gtk_adjustment_get_upper(adjustment);
    
    if (!(value >= minVal)) value = minVal;
    if (value > maxVal) value = maxVal;
    
    ng_linux_set_quiet(slider, 1);
    gtk_range_set_value(range, value);
    ng_linux_set_quiet(slider, 0);
    return NG_SUCCESS;
}

double ng_linux_slider_get_value(NGHandle slider) {
    if (!slider) return 0.0;
    
    return gtk_range_get_value(GTK_RANGE(slider));
}

int ng_linux_slider_set_enabled(NGHandle slider, int enabled) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    gtk_widget_set_sensitive((GtkWidget*)slider, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_slider_invalidate(NGHandle slider) {
    if (!slider) return;
    gtk_widget_queue_draw((GtkWidget*)slider);
}

