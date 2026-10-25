#ifndef AUREA_WINDOWS_OCCLUSION_H
#define AUREA_WINDOWS_OCCLUSION_H

#include <windows.h>

/* As ng_platform_window_occlusion returns them. */
#define AUREA_OCCLUSION_VISIBLE 0
#define AUREA_OCCLUSION_PARTLY 1
#define AUREA_OCCLUSION_COVERED 2

/* How much other windows cover `hwnd`. */
int ng_windows_occlusion_of(HWND hwnd);
/* The level for `visible` of `total` pixels: visible above 99.9%, covered below 0.1%. */
int ng_windows_occlusion_level(LONGLONG visible, LONGLONG total);

#endif
