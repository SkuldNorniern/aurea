#ifndef NATIVE_GUI_TYPES_H
#define NATIVE_GUI_TYPES_H

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

/* What a file dialog picks. */
#define NG_DIALOG_OPEN 0
#define NG_DIALOG_SAVE 1
#define NG_DIALOG_FOLDER 2

/* What a file drop callback reports. */
#define NG_DROP_HOVER 0
#define NG_DROP_DROP 1
#define NG_DROP_LEAVE 2

#endif // NATIVE_GUI_TYPES_H 