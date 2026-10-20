#ifndef NATIVE_GUI_LINUX_ELEMENTS_H
#define NATIVE_GUI_LINUX_ELEMENTS_H

#include "common/types.h"
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_button(const char* title, unsigned int id);
void ng_linux_button_invalidate(NGHandle button);
NGHandle ng_linux_create_label(const char* text);
void ng_linux_label_invalidate(NGHandle label);
int ng_linux_label_set_text(NGHandle label, const char* text);
int ng_linux_label_set_style(NGHandle label, int style);
NGHandle ng_linux_create_box(int is_vertical);
void ng_linux_box_invalidate(NGHandle box);
int ng_linux_box_add(NGHandle box, NGHandle element, float weight);
int ng_linux_box_set_spacing(NGHandle box, int spacing);
int ng_linux_box_set_padding(NGHandle box, int padding);
int ng_linux_element_set_change_id(NGHandle element, unsigned int id);
int ng_linux_element_set_visible(NGHandle element, int visible);
int ng_linux_element_set_min_size(NGHandle element, int width, int height);
int ng_linux_element_set_enabled(NGHandle element, int enabled);
int ng_linux_element_watch_destroy(NGHandle element, unsigned int id);
int ng_linux_text_unit(void);
int ng_linux_text_get_selection(NGHandle text, int* start, int* end);
int ng_linux_text_set_selection(NGHandle text, int start, int end);
int ng_linux_text_replace(NGHandle text, int start, int end, const char* replacement);
int ng_linux_text_undo(NGHandle text, int redo);
int ng_linux_text_can_undo(NGHandle text, int redo);
int ng_linux_text_set_feature(NGHandle text, int feature, int on);
void ng_linux_set_quiet(void* object, int quiet);
int ng_linux_is_quiet(void* object);
void ng_linux_undo_attach(void* text_buffer);
void ng_linux_undo_keys(void* text_view);
int ng_linux_undo(void* text_buffer, int redo);
int ng_linux_can_undo(void* text_buffer, int redo);
void ng_linux_undo_clear(void* text_buffer);
int ng_linux_undo_enable(void* text_buffer, int on);
NGHandle ng_linux_create_text_area(int editable);
NGHandle ng_linux_create_split_view(int is_vertical);
int ng_linux_split_view_add(NGHandle split_handle, NGHandle element);
int ng_linux_split_view_set_divider_position(NGHandle split_handle, int index, float position);
NGHandle ng_linux_create_scroll_view(void);
int ng_linux_scroll_view_set_content(NGHandle scroll, NGHandle element);
NGHandle ng_linux_create_text_editor(void);
void ng_linux_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_linux_create_text_view(int is_editable);
void ng_linux_text_view_invalidate(NGHandle text_view);
NGHandle ng_linux_create_text_field(void);
int ng_linux_set_text_content(NGHandle text_handle, const char* content);
char* ng_linux_get_text_content(NGHandle text_handle);
void ng_linux_free_text_content(char* content);
NGHandle ng_linux_create_canvas(int width, int height);
void ng_linux_canvas_invalidate(NGHandle canvas);
void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_linux_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned);
void ng_linux_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void ng_linux_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);
NGHandle ng_linux_canvas_get_window(NGHandle canvas);
NGHandle ng_linux_canvas_get_native_handle(NGHandle canvas);
int ng_linux_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection);
int ng_linux_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display);

NGHandle ng_linux_create_image_view(void);
int ng_linux_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_linux_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_linux_image_view_set_scaling(NGHandle image_view, int scaling_mode);
void ng_linux_image_view_invalidate(NGHandle image_view);

NGHandle ng_linux_create_slider(double min, double max);
int ng_linux_slider_set_value(NGHandle slider, double value);
double ng_linux_slider_get_value(NGHandle slider);
int ng_linux_slider_set_enabled(NGHandle slider, int enabled);
void ng_linux_slider_invalidate(NGHandle slider);

NGHandle ng_linux_create_checkbox(const char* label);
int ng_linux_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_linux_checkbox_get_checked(NGHandle checkbox);
int ng_linux_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_linux_checkbox_invalidate(NGHandle checkbox);

NGHandle ng_linux_create_progress_bar(void);
int ng_linux_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_linux_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_linux_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_linux_progress_bar_invalidate(NGHandle progress_bar);

NGHandle ng_linux_create_combo_box(void);
int ng_linux_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_linux_combo_box_set_selected(NGHandle combo_box, int index);
int ng_linux_combo_box_get_selected(NGHandle combo_box);
int ng_linux_combo_box_clear(NGHandle combo_box);
int ng_linux_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_linux_combo_box_invalidate(NGHandle combo_box);

NGHandle ng_linux_create_tab_bar(unsigned int id);
int ng_linux_tab_bar_add_tab(NGHandle tab_bar, const char* title);
int ng_linux_tab_bar_remove_tab(NGHandle tab_bar, int index);
int ng_linux_tab_bar_set_selected(NGHandle tab_bar, int index);
int ng_linux_tab_bar_get_selected(NGHandle tab_bar);
void ng_linux_tab_bar_invalidate(NGHandle tab_bar);

NGHandle ng_linux_create_sidebar_list(unsigned int id);
int ng_linux_sidebar_list_add_section(NGHandle sidebar, const char* title);
int ng_linux_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent);
int ng_linux_sidebar_list_set_selected(NGHandle sidebar, int index);
int ng_linux_sidebar_list_get_selected(NGHandle sidebar);
int ng_linux_sidebar_list_clear(NGHandle sidebar);
void ng_linux_sidebar_list_invalidate(NGHandle sidebar);

#ifdef __cplusplus
}
#endif

#endif
