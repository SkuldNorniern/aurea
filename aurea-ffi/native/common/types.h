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

#endif