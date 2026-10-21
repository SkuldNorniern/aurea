#ifndef NATIVE_GUI_WINDOWS_DIALOG_H
#define NATIVE_GUI_WINDOWS_DIALOG_H

#include "common/types.h"
#include <wchar.h>

#ifdef __cplusplus
extern "C" {
#endif

char* ng_windows_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                             const char* default_name, const char* directory, int multiple);
/* An OPENFILENAME result, one path or a folder then its names, as a NUL-ended path list. */
char* ng_windows_legacy_dialog_paths(const wchar_t* buffer);
int ng_windows_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons);
int ng_windows_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out);

#ifdef __cplusplus
}
#endif

#endif
