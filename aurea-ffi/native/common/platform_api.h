#ifndef AUREA_PLATFORM_API_H
#define AUREA_PLATFORM_API_H

#include "types.h"
#include <stdint.h>
#include <stddef.h>

#define AUREA_FFI_ABI_VERSION 2

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ScaleFactorCallback)(void*, float);

int ng_platform_get_abi_version(void);

int ng_platform_init(void);
int ng_platform_reduced_motion(void);
/* 1 when the system appearance is dark, 0 light, -1 when unknown. */
int ng_platform_prefers_dark(void);
/* Lines a wheel notch scrolls, -1 for a page; 3 where the system has no setting. */
int ng_platform_wheel_lines(void);
/* The system's alert sound. */
void ng_platform_beep(void);
int ng_platform_locales(char* buf, int cap);
void ng_platform_cleanup(void);
int ng_platform_run(void);
int ng_platform_poll_events(void);
int ng_platform_wait_events(int timeout_ms);
void ng_platform_request_frame(void);
void ng_platform_frame_idle(void);

NGHandle ng_platform_create_window(const char* title, int width, int height);
NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type);
void ng_platform_destroy_window(NGHandle handle);
void ng_platform_window_set_title(NGHandle window, const char* title);
int ng_platform_window_set_icon_rgba(
    NGHandle window,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
);
void ng_platform_window_set_size(NGHandle window, int width, int height);
int ng_platform_window_set_min_size(NGHandle window, int width, int height);
void ng_platform_window_get_size(NGHandle window, int* width, int* height);
void ng_platform_window_request_close(NGHandle window);
int ng_platform_window_is_focused(NGHandle window);
int ng_platform_window_set_cursor_visible(NGHandle window, int visible);
int ng_platform_window_set_cursor_grab(NGHandle window, int mode);
int ng_platform_window_set_cursor_icon(NGHandle window, int icon);
int ng_platform_window_set_cursor_rgba(NGHandle window, const unsigned char* rgba, unsigned int width, unsigned int height, unsigned int hot_x, unsigned int hot_y);
int ng_platform_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height);
int ng_platform_window_set_shortcuts(NGHandle window, const unsigned int* keycodes, const unsigned int* modifiers, int count);
NGHandle ng_platform_window_get_content_view(NGHandle window);
void ng_platform_window_show(NGHandle window);
void ng_platform_window_hide(NGHandle window);

int ng_platform_window_set_hide_on_close(NGHandle window, int hide);

void ng_platform_window_set_close_guard(NGHandle window, int guarded);

NGHandle ng_platform_create_tray(const char* tooltip, unsigned int click_id);
void ng_platform_destroy_tray(NGHandle tray);
int ng_platform_tray_set_icon_rgba(NGHandle tray, const unsigned char* rgba, unsigned int width, unsigned int height);
int ng_platform_tray_set_tooltip(NGHandle tray, const char* tooltip);
int ng_platform_tray_add_item(NGHandle tray, const char* title, unsigned int id);
int ng_platform_tray_notify(NGHandle tray, const char* title, const char* body);
int ng_platform_window_is_visible(NGHandle window);
/* How much other windows cover `window`: 0 visible, 1 partly covered, 2 covered or minimized. */
int ng_platform_window_occlusion(NGHandle window);
/* The frame's rect while not maximized, x y width height into `rect`, in screen units; returns
 * bit 0 maximized and bit 1 minimized, or a negative error. */
int ng_platform_window_get_placement(NGHandle window, int* rect);
/* Flashes the taskbar entry or bounces the dock icon until the window is in front. */
int ng_platform_window_request_attention(NGHandle window, int critical);
/* Puts the frame at `rect` (kept on a screen), maximized after when asked. */
int ng_platform_window_set_placement(NGHandle window, const int* rect, int maximized);
void ng_platform_window_set_position(NGHandle window, int x, int y);
void ng_platform_window_get_position(NGHandle window, int* x, int* y);
int ng_platform_window_get_xcb_handle(NGHandle window, uint32_t* xcb_window, void** xcb_connection);
int ng_platform_window_get_wayland_handle(NGHandle window, void** surface, void** display);

NGMenuHandle ng_platform_create_menu(void);
void ng_platform_destroy_menu(NGMenuHandle handle);
int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu);
int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_platform_add_menu_separator(NGMenuHandle menu);
NGMenuHandle ng_platform_create_submenu(NGMenuHandle parent_menu, const char* title);
int ng_platform_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio);
int ng_platform_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled);
int ng_platform_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked);
int ng_platform_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title);
int ng_platform_menu_item_remove(NGMenuHandle menu, unsigned int id);
int ng_platform_menu_clear(NGMenuHandle menu);

/* The chosen paths, each ending in a NUL, with an empty one after the last; NULL when cancelled. */
char* ng_platform_file_dialog(NGHandle window, int kind, const char* title, const char* filters, const char* default_name, const char* directory, int multiple);
void ng_platform_free_dialog_paths(char* paths);

int ng_platform_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons);

int ng_platform_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out);

NGHandle ng_platform_create_button(const char* title, unsigned int id);
void ng_platform_button_invalidate(NGHandle button);
NGHandle ng_platform_create_label(const char* text);
void ng_platform_label_invalidate(NGHandle label);
int ng_platform_label_set_text(NGHandle label, const char* text);
#define NG_TEXT_STYLE_BODY 0
#define NG_TEXT_STYLE_STRONG 1
#define NG_TEXT_STYLE_SMALL 2
#define NG_TEXT_STYLE_HEADING 3
int ng_platform_label_set_style(NGHandle label, int style);
NGHandle ng_platform_create_box(int is_vertical);
void ng_platform_box_invalidate(NGHandle box_handle);
int ng_platform_box_add(NGHandle box_handle, NGHandle element, float weight);
int ng_platform_set_window_content(NGHandle window, NGHandle content);

void ng_platform_destroy_element(NGHandle element);

int ng_platform_detach_element(NGHandle element);

void ng_platform_element_released_to_parent(NGHandle element);
void ng_platform_element_retain(NGHandle element);
void ng_platform_element_release(NGHandle element);
int ng_platform_element_set_change_id(NGHandle element, unsigned int id);
int ng_platform_element_set_visible(NGHandle element, int visible);
int ng_platform_element_set_min_size(NGHandle element, int width, int height);
int ng_platform_element_set_accessible_name(NGHandle element, const char* name);
int ng_platform_box_set_spacing(NGHandle box, int spacing);
int ng_platform_box_set_padding(NGHandle box, int padding);
NGHandle ng_platform_creation_parent(void);
int ng_platform_element_set_enabled(NGHandle element, int enabled);
int ng_platform_element_watch_destroy(NGHandle element, unsigned int id);
#define NG_TEXT_UNIT_UTF16 0
#define NG_TEXT_UNIT_CHAR 1
#define NG_TEXT_FEATURE_READ_ONLY 0
#define NG_TEXT_FEATURE_WRAP 1
#define NG_TEXT_FEATURE_MONOSPACE 2
#define NG_TEXT_FEATURE_UNDO 3
#define NG_TEXT_FEATURE_SECRET 4
int ng_platform_text_unit(void);
int ng_platform_text_get_selection(NGHandle text, int* start, int* end);
int ng_platform_text_set_selection(NGHandle text, int start, int end);
int ng_platform_text_replace(NGHandle text, int start, int end, const char* replacement);
int ng_platform_text_undo(NGHandle text, int redo);
int ng_platform_text_can_undo(NGHandle text, int redo);
int ng_platform_text_set_feature(NGHandle text, int feature, int on);

NGHandle ng_platform_create_split_view(int is_vertical);
int ng_platform_split_view_add(NGHandle split_handle, NGHandle element);
int ng_platform_split_view_set_divider_position(NGHandle split_handle, int index, float position);
NGHandle ng_platform_create_scroll_view(void);
int ng_platform_scroll_view_set_content(NGHandle scroll, NGHandle element);
#define NG_SCROLLBAR_AUTO 0
#define NG_SCROLLBAR_ALWAYS 1
int ng_platform_scroll_view_set_scrollbar(NGHandle scroll, int policy);

NGHandle ng_platform_create_text_editor(void);
void ng_platform_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_platform_create_text_view(int is_editable);
void ng_platform_text_view_invalidate(NGHandle text_view);
NGHandle ng_platform_create_text_field(void);
int ng_platform_set_text_content(NGHandle text_handle, const char* content);
char* ng_platform_get_text_content(NGHandle text_handle);
void ng_platform_free_text_content(char* content);

NGHandle ng_platform_create_canvas(int width, int height);
void ng_platform_canvas_invalidate(NGHandle canvas);
void ng_platform_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_platform_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void* ng_platform_canvas_acquire_buffer(NGHandle canvas, unsigned int width, unsigned int height, unsigned int* stride_px, unsigned int* buffer_index);
void ng_platform_canvas_present(NGHandle canvas);
void ng_platform_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned);
void ng_platform_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);
void ng_platform_canvas_get_pixel_size(NGHandle canvas, unsigned int* width, unsigned int* height);
NGHandle ng_platform_canvas_get_window(NGHandle canvas);
NGHandle ng_platform_canvas_get_native_handle(NGHandle canvas);
int ng_platform_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection);
int ng_platform_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display);

float ng_platform_get_scale_factor(NGHandle window);
void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback);
void ng_platform_window_set_lifecycle_callback(NGHandle window);

NGHandle ng_platform_create_image_view(void);
int ng_platform_image_view_load_from_path(NGHandle image_view, const char* path);
int ng_platform_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size);
void ng_platform_image_view_set_scaling(NGHandle image_view, int scaling_mode);
int ng_platform_image_view_get_size(NGHandle image_view, unsigned int* width, unsigned int* height);
void ng_platform_image_view_invalidate(NGHandle image_view);

NGHandle ng_platform_create_slider(double min, double max);
int ng_platform_slider_set_value(NGHandle slider, double value);
double ng_platform_slider_get_value(NGHandle slider);
int ng_platform_slider_set_enabled(NGHandle slider, int enabled);
void ng_platform_slider_invalidate(NGHandle slider);

NGHandle ng_platform_create_checkbox(const char* label);
int ng_platform_checkbox_set_checked(NGHandle checkbox, int checked);
int ng_platform_checkbox_get_checked(NGHandle checkbox);
int ng_platform_checkbox_set_enabled(NGHandle checkbox, int enabled);
void ng_platform_checkbox_invalidate(NGHandle checkbox);

NGHandle ng_platform_create_progress_bar(void);
int ng_platform_progress_bar_set_value(NGHandle progress_bar, double value);
int ng_platform_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate);
int ng_platform_progress_bar_set_enabled(NGHandle progress_bar, int enabled);
void ng_platform_progress_bar_invalidate(NGHandle progress_bar);

NGHandle ng_platform_create_combo_box(void);
int ng_platform_combo_box_add_item(NGHandle combo_box, const char* item);
int ng_platform_combo_box_set_selected(NGHandle combo_box, int index);
int ng_platform_combo_box_get_selected(NGHandle combo_box);
int ng_platform_combo_box_clear(NGHandle combo_box);
int ng_platform_combo_box_set_enabled(NGHandle combo_box, int enabled);
void ng_platform_combo_box_invalidate(NGHandle combo_box);

NGHandle ng_platform_create_tab_bar(unsigned int id);
int ng_platform_tab_bar_add_tab(NGHandle tab_bar, const char* title);
int ng_platform_tab_bar_remove_tab(NGHandle tab_bar, int index);
int ng_platform_tab_bar_set_selected(NGHandle tab_bar, int index);
int ng_platform_tab_bar_get_selected(NGHandle tab_bar);
void ng_platform_tab_bar_invalidate(NGHandle tab_bar);

NGHandle ng_platform_create_sidebar_list(unsigned int id);
int ng_platform_sidebar_list_add_section(NGHandle sidebar, const char* title);
int ng_platform_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent);
int ng_platform_sidebar_list_set_selected(NGHandle sidebar, int index);
int ng_platform_sidebar_list_get_selected(NGHandle sidebar);
int ng_platform_sidebar_list_clear(NGHandle sidebar);
void ng_platform_sidebar_list_invalidate(NGHandle sidebar);

NGHandle ng_platform_create_table(unsigned int id);
int ng_platform_table_add_column(NGHandle table, const char* title);
int ng_platform_table_insert_row(NGHandle table, int index, const char* const* cells, int count);
int ng_platform_table_set_row(NGHandle table, int index, const char* const* cells, int count);
int ng_platform_table_remove_row(NGHandle table, int index);
int ng_platform_table_clear(NGHandle table);
int ng_platform_table_set_selected(NGHandle table, int index);
int ng_platform_table_get_selected(NGHandle table);

NGHandle ng_platform_create_row_list(unsigned int id);
int ng_platform_row_list_insert(NGHandle list, int index, NGHandle row);
int ng_platform_row_list_remove(NGHandle list, int index);
int ng_platform_row_list_clear(NGHandle list);
int ng_platform_row_list_set_selected(NGHandle list, int index);
int ng_platform_row_list_get_selected(NGHandle list);

NGHandle ng_platform_create_swiftui_host(int width, int height);

typedef struct NGClipboardEntry {
    const char* format;
    const unsigned char* data;
    size_t len;
} NGClipboardEntry;
int ng_platform_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len);
int ng_platform_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len);
int ng_platform_clipboard_write(const NGClipboardEntry* entries, size_t count);
void ng_platform_clipboard_free(unsigned char* data);
char* ng_platform_get_clipboard_text(void);
void ng_platform_free_clipboard_text(char* text);
int ng_platform_set_clipboard_text(const char* text);

#ifdef __cplusplus
}
#endif

#endif
