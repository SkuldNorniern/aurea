#include "native_gui.h"

#ifdef _WIN32
#include <windows.h>
#include <stdlib.h>

static HMENU hMenuBar = NULL;

int ngui_create_menu_bar() {
    // Create a menu bar using the WinAPI.
    hMenuBar = CreateMenu();
    if (!hMenuBar) {
        return -1; // Error: Could not create the menu.
    }
    // Note: In a full implementation, you would attach the menu bar to a specific window.
    return 0;
}

int ngui_add_menu_item(const char* title, void (*callback)(void)) {
    // Template Windows implementation:
    // In a proper implementation, you would need to store the callback pointer
    // and associate it with the menu item identifier for later invocation.
    MENUITEMINFOA mii;
    ZeroMemory(&mii, sizeof(MENUITEMINFOA));
    mii.cbSize = sizeof(MENUITEMINFOA);
    mii.fMask = MIIM_STRING | MIIM_ID;
    mii.wID = 1; // Dummy identifier; in a real implementation, assign unique IDs.
    mii.dwTypeData = (LPSTR)title;

    if (!InsertMenuItemA(hMenuBar, 0, TRUE, &mii)) {
        return -1;
    }
    return 0;
}

void ngui_destroy_menu_bar() {
    if (hMenuBar) {
        DestroyMenu(hMenuBar);
        hMenuBar = NULL;
    }
}

#elif defined(__APPLE__)
#include <stdio.h>

int ngui_create_menu_bar() {
    // TODO: Implement native menubar creation using Cocoa on macOS.
    printf("ngui_create_menu_bar: macOS stub implementation\n");
    return 0;
}

int ngui_add_menu_item(const char *title, void (*callback)(void)) {
    // TODO: Implement menu item addition using Cocoa on macOS.
    printf("ngui_add_menu_item: macOS stub implementation: %s\n", title);
    return 0;
}

void ngui_destroy_menu_bar() {
    // Stub for macOS.
    printf("ngui_destroy_menu_bar: macOS stub implementation\n");
}

#else // Linux and others
#include <stdio.h>

int ngui_create_menu_bar() {
    // On Linux, native menubar support depends on the desktop environment.
    // For example, integration with GTK or Qt might be necessary.
    // Here we provide a stub implementation.
    printf("ngui_create_menu_bar: Linux stub implementation\n");
    return 0;
}

int ngui_add_menu_item(const char *title, void (*callback)(void)) {
    printf("ngui_add_menu_item: Linux stub implementation: %s\n", title);
    return 0;
}

void ngui_destroy_menu_bar() {
    // Stub for Linux.
    printf("ngui_destroy_menu_bar: Linux stub implementation\n");
}
#endif 