#ifndef NATIVE_GUI_IOS_ELEMENTS_H
#define NATIVE_GUI_IOS_ELEMENTS_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_ios_create_button_impl(const char* title, unsigned int id);
NGHandle ng_ios_create_label_impl(const char* text);
NGHandle ng_ios_create_box(int is_vertical);
int ng_ios_box_add(NGHandle box, NGHandle element);
NGHandle ng_ios_create_canvas_impl(int width, int height);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_ELEMENTS_H

