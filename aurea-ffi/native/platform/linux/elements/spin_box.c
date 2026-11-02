#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_spin_box(double min, double max, double step, double value) {
    GtkWidget* spin = gtk_spin_button_new_with_range(min, max < min ? min : max, step > 0.0 ? step : 1.0);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(spin), value);
    gtk_widget_show(spin);
    return (NGHandle)spin;
}

double ng_linux_spin_box_get_value(NGHandle spin_box) {
    if (!spin_box || !GTK_IS_SPIN_BUTTON(spin_box)) return 0.0;
    /* What was typed only reaches the value on Enter or a focus change. */
    ng_linux_set_quiet(spin_box, 1);
    gtk_spin_button_update(GTK_SPIN_BUTTON(spin_box));
    ng_linux_set_quiet(spin_box, 0);
    return gtk_spin_button_get_value(GTK_SPIN_BUTTON(spin_box));
}

int ng_linux_spin_box_set_value(NGHandle spin_box, double value) {
    if (!spin_box || !GTK_IS_SPIN_BUTTON(spin_box)) return NG_ERROR_INVALID_HANDLE;
    ng_linux_set_quiet(spin_box, 1);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(spin_box), value);
    ng_linux_set_quiet(spin_box, 0);
    return NG_SUCCESS;
}
