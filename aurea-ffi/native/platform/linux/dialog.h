#ifndef NATIVE_GUI_LINUX_DIALOG_H
#define NATIVE_GUI_LINUX_DIALOG_H

#include "common/types.h"

char* ng_linux_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                           const char* default_name, const char* directory, int multiple);
int ng_linux_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons);
int ng_linux_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out);

#endif
