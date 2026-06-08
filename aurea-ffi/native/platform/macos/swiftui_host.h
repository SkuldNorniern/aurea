#ifndef AUREA_MACOS_SWIFTUI_HOST_H
#define AUREA_MACOS_SWIFTUI_HOST_H

#include "../../common/platform_api.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_macos_try_create_swiftui_host(int width, int height);

#ifdef __cplusplus
}
#endif

#endif
