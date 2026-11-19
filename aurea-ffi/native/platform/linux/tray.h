#ifndef AUREA_LINUX_TRAY_H
#define AUREA_LINUX_TRAY_H

#include "common/platform_api.h"

NGHandle ng_linux_create_tray(const char* tooltip, unsigned int click_id);
void ng_linux_destroy_tray(NGHandle tray);
int ng_linux_tray_set_icon_rgba(NGHandle tray, const unsigned char* rgba, unsigned int width, unsigned int height);
int ng_linux_tray_set_tooltip(NGHandle tray, const char* tooltip);
int ng_linux_tray_add_item(NGHandle tray, const char* title, unsigned int id);
int ng_linux_tray_notify(NGHandle tray, const char* title, const char* body);

#endif
