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

/* How a message box looks, and which buttons it has. */
#define NG_MESSAGE_INFO 0
#define NG_MESSAGE_WARNING 1
#define NG_MESSAGE_ERROR 2

#define NG_MESSAGE_BUTTONS_OK 0
#define NG_MESSAGE_BUTTONS_OK_CANCEL 1
#define NG_MESSAGE_BUTTONS_YES_NO 2
#define NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL 3

/* What a message box returns. */
#define NG_CHOICE_OK 0
#define NG_CHOICE_CANCEL 1
#define NG_CHOICE_YES 2
#define NG_CHOICE_NO 3
#define NG_CHOICE_SAVE 4
#define NG_CHOICE_DISCARD 5

/* What a file drop callback reports. */
#define NG_DROP_HOVER 0
#define NG_DROP_DROP 1
#define NG_DROP_LEAVE 2

#endif