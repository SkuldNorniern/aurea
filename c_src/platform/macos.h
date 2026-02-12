#ifndef NATIVE_GUI_MACOS_H
#define NATIVE_GUI_MACOS_H

#include "common/types.h"

#ifdef __OBJC__
@class NSWindow;
@class NSMenu;
@class NSView;
@class NSButton;
@class NSTextField;
@class NSStackView;
#endif

// Platform-specific implementations
#ifdef __cplusplus
extern "C" {
#endif

int ng_platform_init(void);
void ng_platform_cleanup(void);
NGHandle ng_platform_create_window(const char* title, int width, int height);
NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_platform_destroy_window(NGHandle handle);
void ng_platform_window_request_close(NGHandle window);
int ng_platform_window_is_focused(NGHandle window);
int ng_platform_window_set_cursor_visible(NGHandle window, int visible);
int ng_platform_window_set_cursor_grab(NGHandle window, int mode);
NGHandle ng_platform_window_get_content_view(NGHandle window);
void ng_platform_window_show(NGHandle window);
void ng_platform_window_hide(NGHandle window);
int ng_platform_window_is_visible(NGHandle window);
void ng_platform_window_set_title(NGHandle window, const char* title);
void ng_platform_window_set_size(NGHandle window, int width, int height);
void ng_platform_window_get_size(NGHandle window, int* width, int* height);
void ng_platform_window_set_position(NGHandle window, int x, int y);
void ng_platform_window_get_position(NGHandle window, int* x, int* y);
NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_platform_add_menu_separator(NGMenuHandle menu);
NGMenuHandle ng_platform_create_submenu(NGMenuHandle parent_menu, const char* title);
int ng_platform_run(void);
int ng_platform_poll_events(void);

// New element-related functions
NGHandle ng_platform_create_button(const char* title, unsigned int id);
void ng_platform_button_invalidate(NGHandle button);
NGHandle ng_platform_create_label(const char* text);
void ng_platform_label_invalidate(NGHandle label);
NGHandle ng_platform_create_box(int is_vertical);
void ng_platform_box_invalidate(NGHandle box_handle);
int ng_platform_box_add(NGHandle box, NGHandle element, float weight);
int ng_platform_set_window_content(NGHandle window, NGHandle content);

// SplitView elements
NGHandle ng_platform_create_split_view(int is_vertical);
int ng_platform_split_view_add(NGHandle split_handle, NGHandle element);
int ng_platform_split_view_set_divider_position(NGHandle split_handle, int index, float position);

// New text-related functions
NGHandle ng_platform_create_text_editor(unsigned int id);
void ng_platform_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_platform_create_text_view(int is_editable, unsigned int id);
void ng_platform_text_view_invalidate(NGHandle text_view);
NGHandle ng_platform_create_text_field(void);
int ng_platform_set_text_content(NGHandle text_handle, const char* content);
char* ng_platform_get_text_content(NGHandle text_handle);
void ng_platform_free_text_content(char* content);

// Canvas functions
NGHandle ng_platform_create_canvas(int width, int height);
void ng_platform_canvas_invalidate(NGHandle canvas);
void ng_platform_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_platform_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void ng_platform_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);
NGHandle ng_platform_canvas_get_window(NGHandle canvas);
NGHandle ng_platform_canvas_get_native_handle(NGHandle canvas);
float ng_platform_get_scale_factor(NGHandle window);
typedef void (*ScaleFactorCallback)(void*, float);
void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_platform_window_set_lifecycle_callback(NGHandle window);

// ImageView functions
NGHandle ng_platform_create_image_view(void);
int ng_platform_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_platform_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_platform_image_view_set_scaling(NGHandle image_view, int scaling_mode);
void ng_platform_image_view_invalidate(NGHandle image_view);

// Slider functions
NGHandle ng_platform_create_slider(double min, double max);
int ng_platform_slider_set_value(NGHandle slider, double value);
double ng_platform_slider_get_value(NGHandle slider);
int ng_platform_slider_set_enabled(NGHandle slider, int enabled);
void ng_platform_slider_invalidate(NGHandle slider);

// Checkbox functions
NGHandle ng_platform_create_checkbox(const char* label);
int ng_platform_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_platform_checkbox_get_checked(NGHandle checkbox);
int ng_platform_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_platform_checkbox_invalidate(NGHandle checkbox);

// ProgressBar functions
NGHandle ng_platform_create_progress_bar(void);
int ng_platform_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_platform_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_platform_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_platform_progress_bar_invalidate(NGHandle progress_bar);

// ComboBox functions
NGHandle ng_platform_create_combo_box(void);
int ng_platform_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_platform_combo_box_set_selected(NGHandle combo_box, int index);
int ng_platform_combo_box_get_selected(NGHandle combo_box);
int ng_platform_combo_box_clear(NGHandle combo_box);
int ng_platform_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_platform_combo_box_invalidate(NGHandle combo_box);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_H 
