/* Open, save and folder dialogs through the shell's IFileDialog. */

#define COBJMACROS
#include "dialog.h"
#include "utils.h"
#include <windows.h>
#include <shobjidl.h>
#include <stdlib.h>
#include <string.h>

#define MAX_FILTERS 32

/* One filter line, "Images|png;jpg", as a name and a "*.png;*.jpg" spec. */
typedef struct {
    wchar_t* name;
    wchar_t* spec;
} Filter;

static wchar_t* widen_n(const char* s, int len) {
    int n = MultiByteToWideChar(CP_UTF8, 0, s, len, NULL, 0);
    wchar_t* out = (wchar_t*)malloc(((size_t)n + 1) * sizeof(wchar_t));
    if (!out) return NULL;
    MultiByteToWideChar(CP_UTF8, 0, s, len, out, n);
    out[n] = L'\0';
    return out;
}

/* "png;jpg" as "*.png;*.jpg"; "*" stays "*.*". */
static wchar_t* spec_of(const char* exts, int len) {
    size_t cap = (size_t)len * 3 + 8;
    char* buf = (char*)malloc(cap);
    if (!buf) return NULL;
    size_t at = 0;
    const char* p = exts;
    const char* end = exts + len;
    while (p < end) {
        const char* semi = memchr(p, ';', (size_t)(end - p));
        const char* stop = semi ? semi : end;
        if (stop > p) {
            if (at) buf[at++] = ';';
            if (stop - p == 1 && *p == '*') {
                memcpy(buf + at, "*.*", 3);
                at += 3;
            } else {
                buf[at++] = '*';
                buf[at++] = '.';
                memcpy(buf + at, p, (size_t)(stop - p));
                at += (size_t)(stop - p);
            }
        }
        p = stop + 1;
    }
    wchar_t* wide = widen_n(buf, (int)at);
    free(buf);
    return wide;
}

static int parse_filters(const char* filters, Filter* out) {
    int count = 0;
    const char* line = filters;
    while (line && *line && count < MAX_FILTERS) {
        const char* nl = strchr(line, '\n');
        int len = nl ? (int)(nl - line) : (int)strlen(line);
        const char* bar = memchr(line, '|', (size_t)len);
        if (bar) {
            out[count].name = widen_n(line, (int)(bar - line));
            out[count].spec = spec_of(bar + 1, (int)(line + len - bar - 1));
            if (out[count].name && out[count].spec) count++;
        }
        line = nl ? nl + 1 : NULL;
    }
    return count;
}

/* Appends `item`'s file system path, and a newline, to `*out`. */
static void append_path(IShellItem* item, char** out, size_t* len) {
    wchar_t* path = NULL;
    if (FAILED(IShellItem_GetDisplayName(item, SIGDN_FILESYSPATH, &path)) || !path) return;
    int n = WideCharToMultiByte(CP_UTF8, 0, path, -1, NULL, 0, NULL, NULL);
    char* grown = (char*)realloc(*out, *len + (size_t)n + 1);
    if (grown) {
        *out = grown;
        if (*len) (*out)[(*len)++] = '\n';
        WideCharToMultiByte(CP_UTF8, 0, path, -1, *out + *len, n, NULL, NULL);
        *len += (size_t)n - 1;
    }
    CoTaskMemFree(path);
}

static void set_folder(IFileDialog* dialog, const char* directory) {
    if (!directory || !*directory) return;
    wchar_t* wide = ng_windows_utf8_to_wide(directory);
    if (!wide) return;
    /* The shell's parser takes only backslashes. */
    for (wchar_t* c = wide; *c; c++) {
        if (*c == L'/') *c = L'\\';
    }
    IShellItem* folder = NULL;
    if (SUCCEEDED(SHCreateItemFromParsingName(wide, NULL, &IID_IShellItem, (void**)&folder))) {
        IFileDialog_SetFolder(dialog, folder);
        IShellItem_Release(folder);
    }
    free(wide);
}

static char* collect(IFileDialog* dialog, int kind, int multiple) {
    char* out = NULL;
    size_t len = 0;
    if (kind == NG_DIALOG_OPEN && multiple) {
        IShellItemArray* items = NULL;
        IFileOpenDialog* open = (IFileOpenDialog*)dialog;
        if (SUCCEEDED(IFileOpenDialog_GetResults(open, &items))) {
            DWORD n = 0;
            IShellItemArray_GetCount(items, &n);
            for (DWORD i = 0; i < n; i++) {
                IShellItem* item = NULL;
                if (SUCCEEDED(IShellItemArray_GetItemAt(items, i, &item))) {
                    append_path(item, &out, &len);
                    IShellItem_Release(item);
                }
            }
            IShellItemArray_Release(items);
        }
    } else {
        IShellItem* item = NULL;
        if (SUCCEEDED(IFileDialog_GetResult(dialog, &item))) {
            append_path(item, &out, &len);
            IShellItem_Release(item);
        }
    }
    return out;
}

char* ng_windows_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                             const char* default_name, const char* directory, int multiple) {
    HRESULT init = CoInitializeEx(NULL, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    IFileDialog* dialog = NULL;
    HRESULT made = kind == NG_DIALOG_SAVE
        ? CoCreateInstance(&CLSID_FileSaveDialog, NULL, CLSCTX_INPROC_SERVER, &IID_IFileSaveDialog, (void**)&dialog)
        : CoCreateInstance(&CLSID_FileOpenDialog, NULL, CLSCTX_INPROC_SERVER, &IID_IFileOpenDialog, (void**)&dialog);
    char* result = NULL;
    if (SUCCEEDED(made) && dialog) {
        FILEOPENDIALOGOPTIONS options = 0;
        IFileDialog_GetOptions(dialog, &options);
        options |= FOS_FORCEFILESYSTEM;
        if (kind == NG_DIALOG_FOLDER) options |= FOS_PICKFOLDERS;
        if (kind == NG_DIALOG_OPEN && multiple) options |= FOS_ALLOWMULTISELECT;
        if (kind == NG_DIALOG_SAVE) options |= FOS_OVERWRITEPROMPT;
        IFileDialog_SetOptions(dialog, options);

        if (title && *title) {
            wchar_t* wide = ng_windows_utf8_to_wide(title);
            if (wide) {
                IFileDialog_SetTitle(dialog, wide);
                free(wide);
            }
        }
        Filter list[MAX_FILTERS];
        int count = kind == NG_DIALOG_FOLDER ? 0 : parse_filters(filters, list);
        if (count > 0) {
            COMDLG_FILTERSPEC specs[MAX_FILTERS];
            for (int i = 0; i < count; i++) {
                specs[i].pszName = list[i].name;
                specs[i].pszSpec = list[i].spec;
            }
            IFileDialog_SetFileTypes(dialog, (UINT)count, specs);
            /* A typed name without one gets the first filter's extension. */
            const wchar_t* ext = list[0].spec;
            if (kind == NG_DIALOG_SAVE && wcsncmp(ext, L"*.", 2) == 0 && wcscmp(ext, L"*.*") != 0) {
                wchar_t first[32] = {0};
                wcsncpy(first, ext + 2, 31);
                wchar_t* semi = wcschr(first, L';');
                if (semi) *semi = L'\0';
                IFileDialog_SetDefaultExtension(dialog, first);
            }
        }
        if (default_name && *default_name) {
            wchar_t* wide = ng_windows_utf8_to_wide(default_name);
            if (wide) {
                IFileDialog_SetFileName(dialog, wide);
                free(wide);
            }
        }
        set_folder(dialog, directory);

        /* The dialog runs its own message loop; frames keep coming. */
        HWND owner = (HWND)window;
        if (owner) ng_windows_modal_begin(owner);
        HRESULT shown = IFileDialog_Show(dialog, owner);
        if (owner) ng_windows_modal_end(owner);
        if (SUCCEEDED(shown)) result = collect(dialog, kind, multiple);

        for (int i = 0; i < count; i++) {
            free(list[i].name);
            free(list[i].spec);
        }
        IFileDialog_Release(dialog);
    }
    if (SUCCEEDED(init)) CoUninitialize();
    return result;
}
