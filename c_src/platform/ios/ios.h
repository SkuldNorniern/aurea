#ifndef NATIVE_GUI_IOS_H
#define NATIVE_GUI_IOS_H

#include "../../common/types.h"

#ifdef __cplusplus
extern "C" {
#endif

// iOS platform initialization
int ng_ios_init(void);
void ng_ios_cleanup(void);

// iOS window management
NGHandle ng_ios_create_window(const char* title, int width, int height);
void ng_ios_destroy_window(NGHandle handle);

// iOS menu management (limited on iOS)
NGMenuHandle ng_ios_create_menu(void);
void ng_ios_destroy_menu(NGMenuHandle handle);

// iOS elements
NGHandle ng_ios_create_button(const char* title);
NGHandle ng_ios_create_label(const char* text);
NGHandle ng_ios_create_canvas(int width, int height);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_H

