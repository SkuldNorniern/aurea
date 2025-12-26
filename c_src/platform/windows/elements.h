#ifndef NATIVE_GUI_WINDOWS_ELEMENTS_H
#define NATIVE_GUI_WINDOWS_ELEMENTS_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_create_button(const char* title, unsigned int id);
NGHandle ng_windows_create_label(const char* text);
NGHandle ng_windows_create_box(int is_vertical);
int ng_windows_box_add(NGHandle box, NGHandle element);
NGHandle ng_windows_create_text_editor(unsigned int id);
NGHandle ng_windows_create_text_view(int is_editable, unsigned int id);
NGHandle ng_windows_create_text_field(void);
int ng_windows_set_text_content(NGHandle text_handle, const char* content);
char* ng_windows_get_text_content(NGHandle text_handle);
void ng_windows_free_text_content(char* content);
NGHandle ng_windows_create_canvas(int width, int height);
void ng_windows_canvas_invalidate(NGHandle canvas);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_WINDOWS_ELEMENTS_H

