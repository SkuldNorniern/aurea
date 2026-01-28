#ifndef NATIVE_GUI_MACOS_ELEMENTS_H
#define NATIVE_GUI_MACOS_ELEMENTS_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_macos_create_button(const char* title, unsigned int id);
void ng_macos_button_invalidate(NGHandle button);
NGHandle ng_macos_create_label(const char* text);
void ng_macos_label_invalidate(NGHandle label);
NGHandle ng_macos_create_box(int is_vertical);
void ng_macos_box_invalidate(NGHandle box);
int ng_macos_box_add(NGHandle box, NGHandle element, float weight);
NGHandle ng_macos_create_text_editor(unsigned int id);
void ng_macos_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_macos_create_text_view(int is_editable, unsigned int id);
void ng_macos_text_view_invalidate(NGHandle text_view);
int ng_macos_set_text_content(NGHandle text_handle, const char* content);
char* ng_macos_get_text_content(NGHandle text_handle);
void ng_macos_free_text_content(char* content);

// SplitView functions
NGHandle ng_macos_create_split_view(int is_vertical);
int ng_macos_split_view_add(NGHandle split_handle, NGHandle element);
int ng_macos_split_view_set_divider_position(NGHandle split_handle, int index, float position);

// Canvas functions
NGHandle ng_macos_create_canvas(int width, int height);
void ng_macos_canvas_invalidate(NGHandle canvas);
void ng_macos_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void ng_macos_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);
NGHandle ng_macos_canvas_get_window(NGHandle canvas);
NGHandle ng_macos_canvas_get_native_handle(NGHandle canvas);

// ImageView functions
NGHandle ng_macos_create_image_view(void);
int ng_macos_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_macos_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_macos_image_view_set_scaling(NGHandle image_view, int scaling_mode);
void ng_macos_image_view_invalidate(NGHandle image_view);

// Slider functions
NGHandle ng_macos_create_slider(double min, double max);
int ng_macos_slider_set_value(NGHandle slider, double value);
double ng_macos_slider_get_value(NGHandle slider);
int ng_macos_slider_set_enabled(NGHandle slider, int enabled);
void ng_macos_slider_invalidate(NGHandle slider);

// Checkbox functions
NGHandle ng_macos_create_checkbox(const char* label);
int ng_macos_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_macos_checkbox_get_checked(NGHandle checkbox);
int ng_macos_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_macos_checkbox_invalidate(NGHandle checkbox);

// ProgressBar functions
NGHandle ng_macos_create_progress_bar(void);
int ng_macos_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_macos_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_macos_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_macos_progress_bar_invalidate(NGHandle progress_bar);

// ComboBox functions
NGHandle ng_macos_create_combo_box(void);
int ng_macos_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_macos_combo_box_set_selected(NGHandle combo_box, int index);
int ng_macos_combo_box_get_selected(NGHandle combo_box);
int ng_macos_combo_box_clear(NGHandle combo_box);
int ng_macos_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_macos_combo_box_invalidate(NGHandle combo_box);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_ELEMENTS_H 