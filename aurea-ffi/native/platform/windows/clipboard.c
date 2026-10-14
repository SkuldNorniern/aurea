#include "common/clipboard.h"
#include <windows.h>
#include <stdio.h>
#include <limits.h>

#define EMPTY_FORMATS "application/x-aurea-empty-clipboard-formats-v1"

static int clipboard_open_error(void) {
    return GetLastError() == ERROR_ACCESS_DENIED ? NG_ERROR_BUSY : NG_ERROR_PLATFORM_SPECIFIC;
}

void* ng_windows_clipboard_owner(void) {
    return CreateWindowExW(0, L"STATIC", L"Aurea clipboard", 0,
        0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
}
void ng_windows_clipboard_cleanup(void* owner) {
    if (owner) DestroyWindow((HWND)owner);
}

static UINT format_id(const char* name) {
    if (!strcmp(name, "CF_DIB")) return CF_DIB;
    if (!strcmp(name, "CF_DIBV5")) return CF_DIBV5;
    if (!strcmp(name, "CF_UNICODETEXT")) return CF_UNICODETEXT;
    if (!strcmp(name, "CF_TEXT")) return CF_TEXT;
    if (!strcmp(name, "CF_HDROP")) return CF_HDROP;
    if (!strcmp(name, "CF_OEMTEXT")) return CF_OEMTEXT;
    if (!strcmp(name, "CF_LOCALE")) return CF_LOCALE;
    if (!strcmp(name, "CF_BITMAP") || !strcmp(name, "CF_PALETTE")
        || !strcmp(name, "CF_METAFILEPICT") || !strcmp(name, "CF_ENHMETAFILE")) return 0;
    if (!strncmp(name, "CF_", 3)) {
        char* end = NULL;
        (void)strtoul(name + 3, &end, 10);
        if (end != name + 3 && !*end) return 0;
    }
    if (!strcmp(name, "image/png")) name = "PNG";
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, name, -1, NULL, 0);
    if (n <= 1 || (size_t)n > SIZE_MAX / sizeof(WCHAR)) return 0;
    WCHAR* wide = (WCHAR*)malloc((size_t)n * sizeof(WCHAR));
    if (!wide) return 0;
    UINT id = 0;
    if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, name, -1, wide, n))
        id = RegisterClipboardFormatW(wide);
    free(wide);
    return id;
}

int ng_windows_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len) {
    if (!OpenClipboard(NULL)) return clipboard_open_error();
    int result = NG_SUCCESS;
    UINT id = 0;
    UINT empty_format = format_id(EMPTY_FORMATS);
    SetLastError(ERROR_SUCCESS);
    while ((id = EnumClipboardFormats(id)) != 0) {
        if (id == empty_format) { SetLastError(ERROR_SUCCESS); continue; }
        char name[1024];
        const char* known = NULL;
        switch (id) {
            case CF_DIB: known = "CF_DIB"; break;
            case CF_DIBV5: known = "CF_DIBV5"; break;
            case CF_BITMAP: known = "CF_BITMAP"; break;
            case CF_UNICODETEXT: known = "CF_UNICODETEXT"; break;
            case CF_TEXT: known = "CF_TEXT"; break;
            case CF_HDROP: known = "CF_HDROP"; break;
            case CF_OEMTEXT: known = "CF_OEMTEXT"; break;
            case CF_LOCALE: known = "CF_LOCALE"; break;
        }
        if (known) strcpy_s(name, sizeof(name), known);
        else {
            WCHAR wide[256];
            int n = GetClipboardFormatNameW(id, wide, 256);
            if (n > 0) {
                int bytes = WideCharToMultiByte(CP_UTF8, 0, wide, n, name, sizeof(name) - 1, NULL, NULL);
                if (!bytes) { result = NG_ERROR_PLATFORM_SPECIFIC; break; }
                name[bytes] = 0;
                if (!strcmp(name, "PNG")) strcpy_s(name, sizeof(name), "image/png");
            } else snprintf(name, sizeof(name), "CF_%u", id);
        }
        result = ng_clipboard_append(data, len, name, max_bytes);
        if (result) break;
        SetLastError(ERROR_SUCCESS);
    }
    if (!result && GetLastError() != ERROR_SUCCESS) result = NG_ERROR_PLATFORM_SPECIFIC;
    CloseClipboard();
    return result;
}

static int is_empty_representation(UINT id) {
    HANDLE handle = GetClipboardData(format_id(EMPTY_FORMATS));
    if (!handle) return 0;
    SIZE_T bytes = GlobalSize(handle);
    if (bytes < 8) return 0;
    const UINT* meta = (const UINT*)GlobalLock(handle);
    if (!meta) return 0;
    int empty = 0;
    UINT count = meta[1];
    if (meta[0] == 1 && count <= (bytes - 8) / sizeof(UINT) && count <= (UINT)CountClipboardFormats())
        for (UINT i = 0; i < count; ++i) if (meta[i + 2] == id) { empty = 1; break; }
    GlobalUnlock(handle);
    return empty;
}

int ng_windows_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len) {
    UINT id = format_id(format);
    if (!id) return NG_ERROR_UNSUPPORTED;
    if (!OpenClipboard(NULL)) return clipboard_open_error();
    HANDLE handle = GetClipboardData(id);
    if (!handle) { CloseClipboard(); return NG_SUCCESS; }
    SIZE_T size = GlobalSize(handle);
    if (size == 1 && is_empty_representation(id)) size = 0;
    if (size > max_bytes) { CloseClipboard(); return NG_ERROR_LIMIT_EXCEEDED; }
    void* bytes = size ? GlobalLock(handle) : NULL;
    if (size && !bytes) { CloseClipboard(); return NG_ERROR_PLATFORM_SPECIFIC; }
    unsigned char* copy = (unsigned char*)malloc(size ? size : 1);
    if (copy && size) memcpy(copy, bytes, size);
    if (bytes) GlobalUnlock(handle);
    CloseClipboard();
    if (!copy) return NG_ERROR_PLATFORM_SPECIFIC;
    *data = copy; *len = size;
    return NG_SUCCESS;
}

int ng_windows_clipboard_write(const NGClipboardEntry* entries, size_t count) {
    if (!count) {
        HWND owner = (HWND)ng_windows_clipboard_owner();
        if (!owner) return NG_ERROR_PLATFORM_SPECIFIC;
        int result = NG_ERROR_PLATFORM_SPECIFIC;
        if (OpenClipboard(owner)) { result = EmptyClipboard() ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC; CloseClipboard(); }
        else result = clipboard_open_error();
        ng_windows_clipboard_cleanup(owner);
        return result;
    }
    if (count > SIZE_MAX / sizeof(HGLOBAL) || count > SIZE_MAX / sizeof(UINT))
        return NG_ERROR_INVALID_PARAMETER;
    HGLOBAL* handles = (HGLOBAL*)calloc(count, sizeof(HGLOBAL));
    UINT* formats = (UINT*)calloc(count, sizeof(UINT));
    if (!handles || !formats) { free(handles); free(formats); return NG_ERROR_PLATFORM_SPECIFIC; }
    int result = NG_SUCCESS;
    UINT empty_format = format_id(EMPTY_FORMATS);
    size_t empty_count = 0;
    HGLOBAL empty_meta = NULL;
    for (size_t i = 0; i < count; ++i) {
        formats[i] = entries[i].format ? format_id(entries[i].format) : 0;
        if (!formats[i] || formats[i] == empty_format || (entries[i].len && !entries[i].data)) {
            result = NG_ERROR_INVALID_PARAMETER; break;
        }
        for (size_t k = 0; k < i; ++k) if (formats[k] == formats[i]) result = NG_ERROR_INVALID_PARAMETER;
        if (result) break;
        if (!entries[i].len) ++empty_count;
        handles[i] = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, entries[i].len ? entries[i].len : 1);
        if (!handles[i]) { result = NG_ERROR_PLATFORM_SPECIFIC; break; }
        if (entries[i].len) {
            void* dst = GlobalLock(handles[i]);
            if (!dst) { result = NG_ERROR_PLATFORM_SPECIFIC; break; }
            memcpy(dst, entries[i].data, entries[i].len);
            GlobalUnlock(handles[i]);
        }
    }
    if (!result && empty_count) {
        if (!empty_format || empty_count > UINT_MAX || empty_count > SIZE_MAX / sizeof(UINT) - 2)
            result = NG_ERROR_INVALID_PARAMETER;
        else {
            empty_meta = GlobalAlloc(GMEM_MOVEABLE, (empty_count + 2) * sizeof(UINT));
            UINT* meta = empty_meta ? (UINT*)GlobalLock(empty_meta) : NULL;
            if (!meta) result = NG_ERROR_PLATFORM_SPECIFIC;
            else {
                meta[0] = 1; meta[1] = (UINT)empty_count;
                size_t at = 2;
                for (size_t i = 0; i < count; ++i) if (!entries[i].len) meta[at++] = formats[i];
                GlobalUnlock(empty_meta);
            }
        }
    }
    if (!result) {
        HWND owner = (HWND)ng_windows_clipboard_owner();
        if (!owner) result = NG_ERROR_PLATFORM_SPECIFIC;
        else if (!OpenClipboard(owner)) result = clipboard_open_error();
        else {
            if (!EmptyClipboard()) result = NG_ERROR_PLATFORM_SPECIFIC;
            for (size_t i = 0; !result && i < count; ++i) {
                if (SetClipboardData(formats[i], handles[i])) handles[i] = NULL;
                else result = NG_ERROR_PLATFORM_SPECIFIC;
            }
            if (!result && empty_meta) {
                if (SetClipboardData(empty_format, empty_meta)) empty_meta = NULL;
                else result = NG_ERROR_PLATFORM_SPECIFIC;
            }
            CloseClipboard();
        }
        ng_windows_clipboard_cleanup(owner);
    }
    for (size_t i = 0; i < count; ++i) if (handles[i]) GlobalFree(handles[i]);
    if (empty_meta) GlobalFree(empty_meta);
    free(formats); free(handles);
    return result;
}
