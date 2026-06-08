#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

NGHandle ng_linux_create_combo_box(void) {
    GtkWidget* comboBox = gtk_combo_box_text_new();
    gtk_widget_show(comboBox);
    
    return (NGHandle)comboBox;
}

int ng_linux_combo_box_add_item(NGHandle combo_box, const char* item) {
    if (!combo_box || !item) return NG_ERROR_INVALID_PARAMETER;
    
    gtk_combo_box_text_append_text(GTK_COMBO_BOX_TEXT(combo_box), item);
    return NG_SUCCESS;
}

int ng_linux_combo_box_set_selected(NGHandle combo_box, int index) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    GtkComboBox* comboBox = GTK_COMBO_BOX(combo_box);
    int count = gtk_tree_model_iter_n_children(gtk_combo_box_get_model(comboBox), NULL);
    
    if (index < 0 || index >= count) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    
    gtk_combo_box_set_active(comboBox, index);
    return NG_SUCCESS;
}

int ng_linux_combo_box_get_selected(NGHandle combo_box) {
    if (!combo_box) return -1;
    
    return gtk_combo_box_get_active(GTK_COMBO_BOX(combo_box));
}

int ng_linux_combo_box_clear(NGHandle combo_box) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    GtkComboBoxText* comboBox = GTK_COMBO_BOX_TEXT(combo_box);
    gtk_combo_box_text_remove_all(comboBox);
    return NG_SUCCESS;
}

int ng_linux_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    gtk_widget_set_sensitive((GtkWidget*)combo_box, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

void ng_linux_combo_box_invalidate(NGHandle combo_box) {
    if (!combo_box) return;
    gtk_widget_queue_draw((GtkWidget*)combo_box);
}



