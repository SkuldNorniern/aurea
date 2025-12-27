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
NGHandle ng_linux_canvas_get_window(NGHandle canvas);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_ELEMENTS_H

