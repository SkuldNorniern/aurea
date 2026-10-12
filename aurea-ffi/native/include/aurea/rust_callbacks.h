#ifndef AUREA_RUST_CALLBACKS_H
#define AUREA_RUST_CALLBACKS_H

#ifdef __cplusplus
extern "C" {
#endif

void ng_process_frames(void);

void ng_log_error(const char* msg);
void ng_log_warn(const char* msg);
void ng_log_info(const char* msg);
void ng_log_debug(const char* msg);
void ng_log_trace(const char* msg);

void ng_invoke_menu_callback(unsigned int id);
void ng_invoke_button_callback(unsigned int id);
void ng_invoke_tab_bar_selected(unsigned int id, int index);
void ng_invoke_tab_bar_detach(unsigned int id, int index);
void ng_invoke_sidebar_list_selected(unsigned int id, int index);

void ng_invoke_text_callback(unsigned int id, const char* content);
void ng_invoke_textview_callback(unsigned int id, const char* content);

void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);
void ng_invoke_key_event(void* window, unsigned int keycode, int pressed, unsigned int modifiers);
void ng_invoke_mouse_button(
    void* window,
    int button,
    int pressed,
    unsigned int modifiers,
    double x,
    double y,
    int click_count);
/* buttons: bit n is held button n (0 left, 1 right, 2 middle, 3 back, 4 forward). */
void ng_invoke_mouse_move(void* window, double x, double y, unsigned int buttons, unsigned int modifiers);
void ng_invoke_mouse_wheel(void* window, double delta_x, double delta_y, unsigned int modifiers);
void ng_invoke_text_input(void* window, const char* text);
/* Text an input method is composing; "" ends it. cursor: byte offset of the
   IME caret in text, or -1. Committed text goes to ng_invoke_text_input. */
void ng_invoke_ime_preedit(void* window, const char* text, int cursor);
void ng_invoke_focus_changed(void* window, int focused);
void ng_invoke_cursor_entered(void* window, int entered);
void ng_invoke_raw_mouse_motion(void* window, double delta_x, double delta_y);
/* One pen sample. pressure 0..1; tilts in degrees -90..90; twist 0..360.
   phase: 0 hover, 1 down, 2 move, 3 up. flags: 1 eraser, 2 barrel button. */
void ng_invoke_pen(
    void* window,
    double x,
    double y,
    float pressure,
    float tilt_x,
    float tilt_y,
    float twist,
    unsigned int phase,
    unsigned int flags,
    unsigned int modifiers);
/* Files dragged over a window or dropped on it: kind is NG_DROP_HOVER,
   DROP or LEAVE; x, y in the content's logical pixels; paths one per line. */
void ng_invoke_file_drop(void* window, unsigned int kind, double x, double y, const char* paths);
void ng_invoke_scale_factor_changed(void* window, float scale_factor);
/* A canvas's own size changed, by its window or by the layout around it. */
void ng_invoke_canvas_resized(void* canvas);

#ifdef __cplusplus
}
#endif

#endif
