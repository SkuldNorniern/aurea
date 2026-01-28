#ifndef NATIVE_GUI_WINDOWS_ELEMENTS_H
#define NATIVE_GUI_WINDOWS_ELEMENTS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_button(const char* title, unsigned int id);
void ng_windows_button_invalidate(NGHandle button);
NGHandle ng_windows_create_label(const char* text);
void ng_windows_label_invalidate(NGHandle label);
NGHandle ng_windows_create_box(int is_vertical);
void ng_windows_box_invalidate(NGHandle box);
int ng_windows_box_add(NGHandle box, NGHandle element);
NGHandle ng_windows_create_text_editor(unsigned int id);
void ng_windows_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_windows_create_text_view(int is_editable, unsigned int id);
void ng_windows_text_view_invalidate(NGHandle text_view);
NGHandle ng_windows_create_text_field(void);
int ng_windows_set_text_content(NGHandle text_handle, const char* content);
char* ng_windows_get_text_content(NGHandle text_handle);
void ng_windows_free_text_content(char* content);
NGHandle ng_windows_create_canvas(int width, int height);
void ng_windows_canvas_invalidate(NGHandle canvas);
void ng_windows_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_windows_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);
NGHandle ng_windows_canvas_get_window(NGHandle canvas);
NGHandle ng_windows_canvas_get_native_handle(NGHandle canvas);
void ng_windows_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);

// ImageView functions
NGHandle ng_windows_create_image_view(void);
int ng_windows_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_windows_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_windows_image_view_set_scaling(NGHandle image_view, int scaling_mode);
void ng_windows_image_view_invalidate(NGHandle image_view);

// Slider functions
NGHandle ng_windows_create_slider(double min, double max);
int ng_windows_slider_set_value(NGHandle slider, double value);
double ng_windows_slider_get_value(NGHandle slider);
int ng_windows_slider_set_enabled(NGHandle slider, int enabled);
void ng_windows_slider_invalidate(NGHandle slider);

// Checkbox functions
NGHandle ng_windows_create_checkbox(const char* label);
int ng_windows_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_windows_checkbox_get_checked(NGHandle checkbox);
int ng_windows_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_windows_checkbox_invalidate(NGHandle checkbox);

// ProgressBar functions
NGHandle ng_windows_create_progress_bar(void);
int ng_windows_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_windows_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_windows_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_windows_progress_bar_invalidate(NGHandle progress_bar);

// ComboBox functions
NGHandle ng_windows_create_combo_box(void);
int ng_windows_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_windows_combo_box_set_selected(NGHandle combo_box, int index);
int ng_windows_combo_box_get_selected(NGHandle combo_box);
int ng_windows_combo_box_clear(NGHandle combo_box);
int ng_windows_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_windows_combo_box_invalidate(NGHandle combo_box);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_ELEMENTS_H

