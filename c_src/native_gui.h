#ifndef NATIVE_GUI_H
#define NATIVE_GUI_H

#ifdef __cplusplus
extern "C" {
#endif

// Platform-specific handle types
#ifdef _WIN32
typedef void* NGHandle;  // Will be HWND
typedef void* NGMenuHandle;  // Will be HMENU
#elif defined(__APPLE__)
typedef void* NGHandle;  // Will be NSWindow*
typedef void* NGMenuHandle;  // Will be NSMenu*
#else
typedef void* NGHandle;  // Will be GtkWindow*
typedef void* NGMenuHandle;  // Will be GtkMenuBar*
#endif

// Core platform-specific functions
NGHandle ng_create_window(const char* title, int width, int height);
void ng_destroy_window(NGHandle handle);
NGMenuHandle ng_create_menu_handle(void);
void ng_destroy_menu_handle(NGMenuHandle handle);
int ng_attach_menu_to_window(NGHandle window, NGMenuHandle menu);
int ng_add_raw_menu_item(NGMenuHandle menu, const char* title, unsigned int id);
int ng_handle_menu_event(NGMenuHandle menu, unsigned int id);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_H 