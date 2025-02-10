#ifndef NATIVE_GUI_H
#define NATIVE_GUI_H

#ifdef __cplusplus
extern "C" {
#endif

// Creates a native menubar.
// Returns 0 on success, or a non-zero error code if it fails.
int ngui_create_menu_bar();

// Adds a menu item to the menubar with the given title and a callback function.
// The callback will be invoked on the corresponding menu action.
// Returns 0 on success, or a non-zero error code if it fails.
int ngui_add_menu_item(const char *title, void (*callback)(void));

// Destroys the native menubar and clears any allocated resources.
void ngui_destroy_menu_bar();

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_H 