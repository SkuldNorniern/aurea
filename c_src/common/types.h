#ifndef NATIVE_GUI_TYPES_H
#define NATIVE_GUI_TYPES_H

// Platform-specific handle types
#ifdef __WIN32__
typedef void* NGHandle;  // Will be HWND
typedef void* NGMenuHandle;  // Will be HMENU
#elif defined(__APPLE__)
typedef void* NGHandle;  // Will be NSWindow*
typedef void* NGMenuHandle;  // Will be NSMenu*
#else
typedef void* NGHandle;  // Will be GtkWindow*
typedef void* NGMenuHandle;  // Will be GtkMenuBar*
#endif

#endif // NATIVE_GUI_TYPES_H 