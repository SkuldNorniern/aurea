#ifndef NATIVE_GUI_IOS_ELEMENTS_H
#define NATIVE_GUI_IOS_ELEMENTS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_ios_create_button_impl(const char* title, unsigned int id);
NGHandle ng_ios_create_label_impl(const char* text);
NGHandle ng_ios_create_box(int is_vertical);
int ng_ios_box_add(NGHandle box, NGHandle element);
NGHandle ng_ios_create_canvas_impl(int width, int height);

// ImageView functions
NGHandle ng_ios_create_image_view(void);
int ng_ios_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_ios_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_ios_image_view_set_scaling(NGHandle image_view, int scaling_mode);

// Slider functions
NGHandle ng_ios_create_slider(double min, double max);
int ng_ios_slider_set_value(NGHandle slider, double value);
double ng_ios_slider_get_value(NGHandle slider);
int ng_ios_slider_set_enabled(NGHandle slider, int enabled);

// Checkbox functions (UISwitch on iOS)
NGHandle ng_ios_create_checkbox(const char* label);
int ng_ios_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_ios_checkbox_get_checked(NGHandle checkbox);
int ng_ios_checkbox_set_enabled(NGHandle checkbox, int enabled);

// ProgressBar functions
NGHandle ng_ios_create_progress_bar(void);
int ng_ios_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_ios_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_ios_progress_bar_set_enabled(NGHandle progress_bar, int enabled);

// ComboBox functions (UIPickerView on iOS)
NGHandle ng_ios_create_combo_box(void);
int ng_ios_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_ios_combo_box_set_selected(NGHandle combo_box, int index);
int ng_ios_combo_box_get_selected(NGHandle combo_box);
int ng_ios_combo_box_clear(NGHandle combo_box);
int ng_ios_combo_box_set_enabled(NGHandle combo_box, int enabled);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_ELEMENTS_H

