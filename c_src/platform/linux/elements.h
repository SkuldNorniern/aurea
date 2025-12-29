#ifndef NATIVE_GUI_LINUX_ELEMENTS_H
#define NATIVE_GUI_LINUX_ELEMENTS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_button(const char* title, unsigned int id);
void ng_linux_button_invalidate(NGHandle button);
NGHandle ng_linux_create_label(const char* text);
void ng_linux_label_invalidate(NGHandle label);
NGHandle ng_linux_create_box(int is_vertical);
void ng_linux_box_invalidate(NGHandle box);
int ng_linux_box_add(NGHandle box, NGHandle element);
NGHandle ng_linux_create_text_editor(unsigned int id);
void ng_linux_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_linux_create_text_view(int is_editable, unsigned int id);
void ng_linux_text_view_invalidate(NGHandle text_view);
int ng_linux_set_text_content(NGHandle text_handle, const char* content);
char* ng_linux_get_text_content(NGHandle text_handle);
void ng_linux_free_text_content(char* content);
NGHandle ng_linux_create_canvas(int width, int height);
void ng_linux_canvas_invalidate(NGHandle canvas);
void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
NGHandle ng_linux_canvas_get_window(NGHandle canvas);

// ImageView functions
NGHandle ng_linux_create_image_view(void);
int ng_linux_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_linux_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_linux_image_view_set_scaling(NGHandle image_view, int scaling_mode);
void ng_linux_image_view_invalidate(NGHandle image_view);

// Slider functions
NGHandle ng_linux_create_slider(double min, double max);
int ng_linux_slider_set_value(NGHandle slider, double value);
double ng_linux_slider_get_value(NGHandle slider);
int ng_linux_slider_set_enabled(NGHandle slider, int enabled);
void ng_linux_slider_invalidate(NGHandle slider);

// Checkbox functions
NGHandle ng_linux_create_checkbox(const char* label);
int ng_linux_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_linux_checkbox_get_checked(NGHandle checkbox);
int ng_linux_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_linux_checkbox_invalidate(NGHandle checkbox);

// ProgressBar functions
NGHandle ng_linux_create_progress_bar(void);
int ng_linux_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_linux_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_linux_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_linux_progress_bar_invalidate(NGHandle progress_bar);

// ComboBox functions
NGHandle ng_linux_create_combo_box(void);
int ng_linux_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_linux_combo_box_set_selected(NGHandle combo_box, int index);
int ng_linux_combo_box_get_selected(NGHandle combo_box);
int ng_linux_combo_box_clear(NGHandle combo_box);
int ng_linux_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_linux_combo_box_invalidate(NGHandle combo_box);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_ELEMENTS_H

