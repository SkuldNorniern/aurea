#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include <gtk/gtk.h>

#define NG_COMBO_TEXT_CHECK(combo) ((combo) && GTK_IS_COMBO_BOX_TEXT(combo))

NGHandle ng_linux_create_combo_box(void) {
    GtkWidget* combo = gtk_combo_box_text_new();
    ng_control_get(combo);
    gtk_widget_show(combo);
    return (NGHandle)combo;
}

/* App changes are not reported, as with the other controls; the first item selects itself. */
int ng_linux_combo_box_add_item(NGHandle combo_box, const char* item) {
    if (!NG_COMBO_TEXT_CHECK(combo_box)) return NG_ERROR_INVALID_HANDLE;
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    ng_linux_set_quiet(combo_box, 1);
    gtk_combo_box_text_append_text(GTK_COMBO_BOX_TEXT(combo_box), item);
    ng_linux_set_quiet(combo_box, 0);
    return NG_SUCCESS;
}

int ng_linux_combo_box_set_selected(NGHandle combo_box, int index) {
    if (!NG_COMBO_TEXT_CHECK(combo_box)) return NG_ERROR_INVALID_HANDLE;
    GtkComboBox* combo = GTK_COMBO_BOX(combo_box);
    int count = gtk_tree_model_iter_n_children(gtk_combo_box_get_model(combo), NULL);
    if (index < 0 || index >= count) return NG_ERROR_INVALID_PARAMETER;

    ng_linux_set_quiet(combo_box, 1);
    gtk_combo_box_set_active(combo, index);
    ng_linux_set_quiet(combo_box, 0);
    return NG_SUCCESS;
}

int ng_linux_combo_box_get_selected(NGHandle combo_box) {
    if (!NG_COMBO_TEXT_CHECK(combo_box)) return -1;
    return gtk_combo_box_get_active(GTK_COMBO_BOX(combo_box));
}

int ng_linux_combo_box_clear(NGHandle combo_box) {
    if (!NG_COMBO_TEXT_CHECK(combo_box)) return NG_ERROR_INVALID_HANDLE;
    ng_linux_set_quiet(combo_box, 1);
    gtk_combo_box_text_remove_all(GTK_COMBO_BOX_TEXT(combo_box));
    ng_linux_set_quiet(combo_box, 0);
    return NG_SUCCESS;
}

int ng_linux_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    if (!combo_box || !GTK_IS_WIDGET(combo_box)) return NG_ERROR_INVALID_HANDLE;
    gtk_widget_set_sensitive((GtkWidget*)combo_box, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_combo_box_invalidate(NGHandle combo_box) {
    if (!combo_box) return;
    gtk_widget_queue_draw((GtkWidget*)combo_box);
}
