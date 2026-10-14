#ifndef AUREA_CLIPBOARD_H
#define AUREA_CLIPBOARD_H
#include "platform_api.h"
#include "errors.h"
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

/* Packed NUL-terminated UTF-8 names, grown only up to the caller's budget. */
static inline int ng_clipboard_append(unsigned char** data, size_t* len,
                                     const char* name, size_t max_bytes) {
    size_t n = strlen(name);
    if (n == SIZE_MAX || *len > max_bytes || n + 1 > max_bytes - *len)
        return NG_ERROR_LIMIT_EXCEEDED;
    unsigned char* fresh = (unsigned char*)realloc(*data, *len + n + 1);
    if (!fresh) return NG_ERROR_PLATFORM_SPECIFIC;
    memcpy(fresh + *len, name, n + 1);
    *data = fresh;
    *len += n + 1;
    return NG_SUCCESS;
}

#define NG_CLIPBOARD_DECL(prefix) \
int prefix##_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len); \
int prefix##_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len); \
int prefix##_clipboard_write(const NGClipboardEntry* entries, size_t count)

#if defined(_WIN32)
NG_CLIPBOARD_DECL(ng_windows);
void* ng_windows_clipboard_owner(void);
void ng_windows_clipboard_cleanup(void* owner);
#elif defined(__APPLE__)
NG_CLIPBOARD_DECL(ng_macos);
#else
NG_CLIPBOARD_DECL(ng_linux);
#endif
#undef NG_CLIPBOARD_DECL
#endif
