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

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_MACOS_ELEMENTS_H 