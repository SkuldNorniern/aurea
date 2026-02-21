#ifndef AUREA_PLATFORM_OPS_H
#define AUREA_PLATFORM_OPS_H

#include "platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ng_platform_ops {
    int (*init)(void);
    void (*cleanup)(void);
    int (*run)(void);
    int (*poll_events)(void);

    NGHandle (*create_window)(const char* title, int width, int height);
    NGHandle (*create_window_with_type)(const char* title, int width, int height, int window_type);
    void (*destroy_window)(NGHandle handle);
    void (*window_set_title)(NGHandle window, const char* title);
    void (*window_set_size)(NGHandle window, int width, int height);
    void (*window_get_size)(NGHandle window, int* width, int* height);
    void (*window_request_close)(NGHandle window);
    int (*window_is_focused)(NGHandle window);
    int (*window_set_cursor_visible)(NGHandle window, int visible);
    int (*window_set_cursor_grab)(NGHandle window, int mode);
    NGHandle (*window_get_content_view)(NGHandle window);
    void (*window_show)(NGHandle window);
    void (*window_hide)(NGHandle window);
    int (*window_is_visible)(NGHandle window);
    void (*window_set_position)(NGHandle window, int x, int y);
    void (*window_get_position)(NGHandle window, int* x, int* y);
    int (*window_get_xcb_handle)(NGHandle window, uint32_t* xcb_window, void** xcb_connection);
    int (*window_get_wayland_handle)(NGHandle window, void** surface, void** display);

    NGMenuHandle (*create_menu)(void);
    void (*destroy_menu)(NGMenuHandle handle);
    int (*attach_menu)(NGHandle window, NGMenuHandle menu);
    int (*add_menu_item)(NGMenuHandle menu, const char* title, unsigned int id);
    int (*add_menu_separator)(NGMenuHandle menu);
    NGMenuHandle (*create_submenu)(NGMenuHandle parent, const char* title);

    NGHandle (*create_button)(const char* title, unsigned int id);
    void (*button_invalidate)(NGHandle button);
    NGHandle (*create_label)(const char* text);
    void (*label_invalidate)(NGHandle label);
    NGHandle (*create_box)(int is_vertical);
    void (*box_invalidate)(NGHandle box);
    int (*box_add)(NGHandle box, NGHandle element, float weight);
    int (*set_window_content)(NGHandle window, NGHandle content);

    NGHandle (*create_split_view)(int is_vertical);
    int (*split_view_add)(NGHandle split, NGHandle element);
    int (*split_view_set_divider_position)(NGHandle split, int index, float position);

    NGHandle (*create_text_editor)(unsigned int id);
    void (*text_editor_invalidate)(NGHandle h);
    NGHandle (*create_text_view)(int is_editable, unsigned int id);
    void (*text_view_invalidate)(NGHandle h);
    NGHandle (*create_text_field)(void);
    int (*set_text_content)(NGHandle h, const char* content);
    char* (*get_text_content)(NGHandle h);
    void (*free_text_content)(char* content);

    NGHandle (*create_canvas)(int width, int height);
    void (*canvas_invalidate)(NGHandle canvas);
    void (*canvas_invalidate_rect)(NGHandle canvas, float x, float y, float w, float h);
    void (*canvas_update_buffer)(NGHandle canvas, const unsigned char* buf, unsigned int size, unsigned int w, unsigned int h);
    void (*canvas_get_size)(NGHandle canvas, unsigned int* w, unsigned int* h);
    NGHandle (*canvas_get_window)(NGHandle canvas);
    NGHandle (*canvas_get_native_handle)(NGHandle canvas);
    int (*canvas_get_xcb_handle)(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection);
    int (*canvas_get_wayland_handle)(NGHandle canvas, void** surface, void** display);

    float (*get_scale_factor)(NGHandle window);
    void (*window_set_scale_factor_callback)(NGHandle window, ScaleFactorCallback callback);
    void (*window_set_lifecycle_callback)(NGHandle window);

    NGHandle (*create_image_view)(void);
    int (*image_view_load_from_path)(NGHandle v, const char* path);
    int (*image_view_load_from_data)(NGHandle v, const unsigned char* data, unsigned int size);
    void (*image_view_set_scaling)(NGHandle v, int mode);
    void (*image_view_invalidate)(NGHandle v);

    NGHandle (*create_slider)(double min, double max);
    int (*slider_set_value)(NGHandle s, double value);
    double (*slider_get_value)(NGHandle s);
    int (*slider_set_enabled)(NGHandle s, int enabled);
    void (*slider_invalidate)(NGHandle s);

    NGHandle (*create_checkbox)(const char* label);
    int (*checkbox_set_checked)(NGHandle c, int checked);
    int (*checkbox_get_checked)(NGHandle c);
    int (*checkbox_set_enabled)(NGHandle c, int enabled);
    void (*checkbox_invalidate)(NGHandle c);

    NGHandle (*create_progress_bar)(void);
    int (*progress_bar_set_value)(NGHandle p, double value);
    int (*progress_bar_set_indeterminate)(NGHandle p, int indeterminate);
    int (*progress_bar_set_enabled)(NGHandle p, int enabled);
    void (*progress_bar_invalidate)(NGHandle p);

    NGHandle (*create_combo_box)(void);
    int (*combo_box_add_item)(NGHandle c, const char* item);
    int (*combo_box_set_selected)(NGHandle c, int index);
    int (*combo_box_get_selected)(NGHandle c);
    int (*combo_box_clear)(NGHandle c);
    int (*combo_box_set_enabled)(NGHandle c, int enabled);
    void (*combo_box_invalidate)(NGHandle c);

    NGHandle (*create_tab_bar)(unsigned int id);
    int (*tab_bar_add_tab)(NGHandle t, const char* title);
    int (*tab_bar_remove_tab)(NGHandle t, int index);
    int (*tab_bar_set_selected)(NGHandle t, int index);
    int (*tab_bar_get_selected)(NGHandle t);
    void (*tab_bar_invalidate)(NGHandle t);

    NGHandle (*create_sidebar_list)(unsigned int id);
    int (*sidebar_list_add_section)(NGHandle s, const char* title);
    int (*sidebar_list_add_item)(NGHandle s, const char* title, int indent);
    int (*sidebar_list_set_selected)(NGHandle s, int index);
    int (*sidebar_list_get_selected)(NGHandle s);
    int (*sidebar_list_clear)(NGHandle s);
    void (*sidebar_list_invalidate)(NGHandle s);

    NGHandle (*create_swiftui_host)(int width, int height);
} ng_platform_ops_t;

void ng_platform_register_ops(const ng_platform_ops_t* ops);

#ifdef __cplusplus
}
#endif

#endif
