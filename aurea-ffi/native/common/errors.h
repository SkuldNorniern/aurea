#ifndef NATIVE_GUI_ERRORS_H
#define NATIVE_GUI_ERRORS_H

/* Error codes. */
#define NG_SUCCESS 0
#define NG_ERROR_INVALID_HANDLE -1
#define NG_ERROR_CREATION_FAILED -2
#define NG_ERROR_INVALID_PARAMETER -3
#define NG_ERROR_PLATFORM_SPECIFIC -4
/* The platform backend has no implementation for this operation. */
#define NG_ERROR_UNSUPPORTED -5
#define NG_ERROR_LIMIT_EXCEEDED -6
#define NG_ERROR_BUSY -7

#endif
