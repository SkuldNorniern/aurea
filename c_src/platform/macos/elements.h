#ifndef NATIVE_GUI_MACOS_ELEMENTS_H
#define NATIVE_GUI_MACOS_ELEMENTS_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_macos_create_button(const char* title, unsigned int id);
void ng_macos_button_invalidate(NGHandle button);
NGHandle ng_macos_create_label(const char* text);
void ng_macos_label_invalidate(NGHandle label);
NGHandle ng_macos_create_box(int is_vertical);
void ng_macos_box_invalidate(NGHandle box);
int ng_macos_box_add(NGHandle box, NGHandle element);
NGHandle ng_macos_create_text_editor(unsigned int id);
void ng_macos_text_editor_invalidate(NGHandle text_editor);
NGHandle ng_macos_create_text_view(int is_editable, unsigned int id);
void ng_macos_text_view_invalidate(NGHandle text_view);
int ng_macos_set_text_content(NGHandle text_handle, const char* content);
char* ng_macos_get_text_content(NGHandle text_handle);
void ng_macos_free_text_content(char* content);

// Canvas functions
NGHandle ng_macos_create_canvas(int width, int height);
void ng_macos_canvas_invalidate(NGHandle canvas);
void ng_macos_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height);
void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height);
void ng_macos_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_ELEMENTS_H 