#ifndef NATIVE_GUI_LINUX_WINDOW_H
#define NATIVE_GUI_LINUX_WINDOW_H

#include "common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

NGHandle ng_linux_create_window(const char* title, int width, int height);
void ng_linux_destroy_window(NGHandle handle);
int ng_linux_set_window_content(NGHandle window, NGHandle content);
float ng_linux_get_scale_factor(NGHandle window);
void ng_linux_window_set_lifecycle_callback(NGHandle window);
void ng_linux_window_set_title(NGHandle window, const char* title);
void ng_linux_window_set_size(NGHandle window, int width, int height);
void ng_linux_window_get_size(NGHandle window, int* width, int* height);
void ng_linux_window_request_close(NGHandle window);
int ng_linux_window_is_focused(NGHandle window);

// Internal function to get main vbox (used by menu.c)
// Note: Returns GtkWidget* but declared as void* to avoid GTK dependency in header
void* ng_linux_get_main_vbox(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_WINDOW_H

