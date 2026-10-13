#ifndef NATIVE_GUI_MACOS_TRAY_H
#define NATIVE_GUI_MACOS_TRAY_H

#include "../../common/types.h"

NGHandle ng_macos_create_tray(const char* tooltip, unsigned int click_id);
void ng_macos_destroy_tray(NGHandle tray);
int ng_macos_tray_set_icon_rgba(NGHandle tray, const unsigned char* rgba, unsigned int width, unsigned int height);
int ng_macos_tray_set_tooltip(NGHandle tray, const char* tooltip);
int ng_macos_tray_add_item(NGHandle tray, const char* title, unsigned int id);
int ng_macos_tray_notify(NGHandle tray, const char* title, const char* body);

#endif
