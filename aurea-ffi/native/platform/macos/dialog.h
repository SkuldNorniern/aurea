#ifndef NATIVE_GUI_MACOS_DIALOG_H
#define NATIVE_GUI_MACOS_DIALOG_H

#include "common/types.h"

char* ng_macos_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                           const char* default_name, const char* directory, int multiple);

#endif
