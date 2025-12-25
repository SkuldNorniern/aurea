#ifndef NATIVE_GUI_MACOS_ELEMENTS_H
#define NATIVE_GUI_MACOS_ELEMENTS_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_macos_create_button(const char* title);
NGHandle ng_macos_create_label(const char* text);
NGHandle ng_macos_create_box(int is_vertical);
int ng_macos_box_add(NGHandle box, NGHandle element);
NGHandle ng_macos_create_text_editor(void);
NGHandle ng_macos_create_text_view(int is_editable);
int ng_macos_set_text_content(NGHandle text_handle, const char* content);
char* ng_macos_get_text_content(NGHandle text_handle);
void ng_macos_free_text_content(char* content);

// Canvas functions
NGHandle ng_macos_create_canvas(int width, int height);
void ng_macos_canvas_invalidate(NGHandle canvas);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_ELEMENTS_H 