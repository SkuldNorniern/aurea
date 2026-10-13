#ifndef NATIVE_GUI_WINDOWS_DIALOG_H
#define NATIVE_GUI_WINDOWS_DIALOG_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

char* ng_windows_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                             const char* default_name, const char* directory, int multiple);
int ng_windows_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons);

#ifdef __cplusplus
}
#endif

#endif
