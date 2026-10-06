#ifndef NATIVE_GUI_WINDOWS_TRAY_H
#define NATIVE_GUI_WINDOWS_TRAY_H

#include "common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_windows_tray_create(const char* tooltip, unsigned int click_id);
void ng_windows_tray_destroy(NGHandle tray);
int ng_windows_tray_set_icon_rgba(
    NGHandle tray,
    const unsigned char* rgba,
    unsigned int width,
    unsigned int height
);
int ng_windows_tray_set_tooltip(NGHandle tray, const char* tooltip);
int ng_windows_tray_add_item(NGHandle tray, const char* title, unsigned int id);
int ng_windows_tray_notify(NGHandle tray, const char* title, const char* body);

#ifdef __cplusplus
}
#endif

#endif
