#ifndef NATIVE_GUI_TYPES_H
#define NATIVE_GUI_TYPES_H

/* Platform-specific handle types. */
#ifdef _WIN32
typedef void* NGHandle;
typedef void* NGMenuHandle;
#elif defined(__APPLE__)
typedef void* NGHandle;
typedef void* NGMenuHandle;
#else
typedef void* NGHandle;
typedef void* NGMenuHandle;
#endif

/* What a file dialog picks. */
#define NG_DIALOG_OPEN 0
#define NG_DIALOG_SAVE 1
#define NG_DIALOG_FOLDER 2

/* What a file drop callback reports. */
#define NG_DROP_HOVER 0
#define NG_DROP_DROP 1
#define NG_DROP_LEAVE 2

#endif