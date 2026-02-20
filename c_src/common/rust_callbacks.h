#ifndef AUREA_RUST_CALLBACKS_H
#define AUREA_RUST_CALLBACKS_H

#ifdef __cplusplus
extern "C" {
#endif

void ng_invoke_menu_callback(unsigned int id);
void ng_invoke_button_callback(unsigned int id);
void ng_invoke_tab_bar_selected(unsigned int id, int index);
void ng_invoke_tab_bar_detach(unsigned int id, int index);
void ng_invoke_sidebar_list_selected(unsigned int id, int index);

void ng_invoke_text_callback(unsigned int id, const char* content);
void ng_invoke_textview_callback(unsigned int id, const char* content);

void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);
void ng_invoke_key_event(void* window, unsigned int keycode, int pressed, unsigned int modifiers);
void ng_invoke_mouse_button(void* window, int button, int pressed, unsigned int modifiers);
void ng_invoke_mouse_move(void* window, double x, double y);
void ng_invoke_mouse_wheel(void* window, double delta_x, double delta_y, unsigned int modifiers);
void ng_invoke_text_input(void* window, const char* text);
void ng_invoke_focus_changed(void* window, int focused);
void ng_invoke_cursor_entered(void* window, int entered);
void ng_invoke_raw_mouse_motion(void* window, double delta_x, double delta_y);
void ng_invoke_scale_factor_changed(void* window, float scale_factor);

#ifdef __cplusplus
}
#endif

#endif
