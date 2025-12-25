#ifndef NATIVE_GUI_LINUX_ELEMENTS_H
#define NATIVE_GUI_LINUX_ELEMENTS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_button(const char* title, unsigned int id);
NGHandle ng_linux_create_label(const char* text);
NGHandle ng_linux_create_box(int is_vertical);
int ng_linux_box_add(NGHandle box, NGHandle element);
NGHandle ng_linux_create_text_editor(void);
NGHandle ng_linux_create_text_view(int is_editable);
int ng_linux_set_text_content(NGHandle text_handle, const char* content);
char* ng_linux_get_text_content(NGHandle text_handle);
void ng_linux_free_text_content(char* content);
NGHandle ng_linux_create_canvas(int width, int height);
void ng_linux_canvas_invalidate(NGHandle canvas);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_ELEMENTS_H

