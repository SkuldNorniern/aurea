#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include <gtk/gtk.h>

static GtkWidget* shown_control(GtkWidget* widget) {
    ng_control_get(widget);
    gtk_widget_show(widget);
    return widget;
}

NGHandle ng_linux_create_checkbox(const char* label) {
    GtkWidget* checkbox = gtk_check_button_new_with_label(label ? label : "");
    return (NGHandle)shown_control(checkbox);
}

NGHandle ng_linux_create_radio(const char* label, NGHandle previous) {
    GtkRadioButton* group = previous && GTK_IS_RADIO_BUTTON(previous) ? GTK_RADIO_BUTTON(previous) : NULL;
    GtkWidget* radio = gtk_radio_button_new_with_label_from_widget(group, label ? label : "");
    return (NGHandle)shown_control(radio);
}

int ng_linux_checkbox_set_checked(NGHandle checkbox, int checked) {
    if (!checkbox || !GTK_IS_TOGGLE_BUTTON(checkbox)) return NG_ERROR_INVALID_HANDLE;
    ng_linux_set_quiet(checkbox, 1);
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(checkbox), checked ? TRUE : FALSE);
    ng_linux_set_quiet(checkbox, 0);
    return NG_SUCCESS;
}

int ng_linux_checkbox_get_checked(NGHandle checkbox) {
    if (!checkbox || !GTK_IS_TOGGLE_BUTTON(checkbox)) return 0;
    return gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(checkbox)) ? 1 : 0;
}

int ng_linux_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    if (!checkbox || !GTK_IS_WIDGET(checkbox)) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_set_sensitive((GtkWidget*)checkbox, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_checkbox_invalidate(NGHandle checkbox) {
    if (!checkbox) return;
    gtk_widget_queue_draw((GtkWidget*)checkbox);
}
