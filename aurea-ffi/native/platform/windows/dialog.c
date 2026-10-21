
#define COBJMACROS
#include "dialog.h"
#include "utils.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <shobjidl.h>
#include <stdlib.h>
#include <string.h>

#define MAX_FILTERS 32

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

static void append_path(IShellItem* item, char** out, size_t* len) {
    wchar_t* path = NULL;
    if (FAILED(IShellItem_GetDisplayName(item, SIGDN_FILESYSPATH, &path)) || !path) return;
    int n = WideCharToMultiByte(CP_UTF8, 0, path, -1, NULL, 0, NULL, NULL);
    /* Each path keeps its NUL, and one more always follows to end the list. */
    char* grown = n > 0 ? (char*)realloc(*out, *len + (size_t)n + 1) : NULL;
    if (grown) {
        *out = grown;
        WideCharToMultiByte(CP_UTF8, 0, path, -1, *out + *len, n, NULL, NULL);
        *len += (size_t)n;
        (*out)[*len] = '\0';
    }
    CoTaskMemFree(path);
}

static void set_folder(IFileDialog* dialog, const char* directory) {
    if (!directory || !*directory) return;
    wchar_t* wide = ng_windows_utf8_to_wide(directory);
    if (!wide) return;
    for (wchar_t* c = wide; *c; c++) {
        if (*c == L'/') *c = L'\\';
    }
    typedef HRESULT (WINAPI *ParseFunc)(PCWSTR, IBindCtx*, REFIID, void**);
    ParseFunc parse = (ParseFunc)ng_windows_api("shell32.dll", "SHCreateItemFromParsingName");
    IShellItem* folder = NULL;
    if (parse && SUCCEEDED(parse(wide, NULL, &IID_IShellItem, (void**)&folder))) {
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

#define ID_SAVE 1001
#define ID_DISCARD 1002

static int choice_of(int button, int buttons) {
    switch (button) {
    case IDOK: return NG_CHOICE_OK;
    case IDYES: return NG_CHOICE_YES;
    case IDNO: return NG_CHOICE_NO;
    case ID_SAVE: return NG_CHOICE_SAVE;
    case ID_DISCARD: return NG_CHOICE_DISCARD;
    default: break;
    }
    if (buttons == NG_MESSAGE_BUTTONS_YES_NO) return NG_CHOICE_NO;
    if (buttons == NG_MESSAGE_BUTTONS_OK) return NG_CHOICE_OK;
    return NG_CHOICE_CANCEL;
}

static TASKDIALOG_COMMON_BUTTON_FLAGS common_buttons_of(int buttons) {
    switch (buttons) {
    case NG_MESSAGE_BUTTONS_OK_CANCEL: return TDCBF_OK_BUTTON | TDCBF_CANCEL_BUTTON;
    case NG_MESSAGE_BUTTONS_YES_NO: return TDCBF_YES_BUTTON | TDCBF_NO_BUTTON;
    case NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL: return TDCBF_CANCEL_BUTTON;
    default: return TDCBF_OK_BUTTON;
    }
}

static const TASKDIALOG_BUTTON save_discard_buttons[] = {
    {ID_SAVE, L"Save"},
    {ID_DISCARD, L"Don't Save"},
};

static PCWSTR icon_of(int kind) {
    switch (kind) {
    case NG_MESSAGE_WARNING: return TD_WARNING_ICON;
    case NG_MESSAGE_ERROR: return TD_ERROR_ICON;
    default: return TD_INFORMATION_ICON;
    }
}

typedef HRESULT (WINAPI* TaskDialogIndirectFn)(const TASKDIALOGCONFIG*, int*, int*, BOOL*);

HANDLE ng_windows_common_controls_v6(void) {
    static HANDLE context = NULL;
    if (context) return context;
    wchar_t dir[MAX_PATH];
    wchar_t shell32[MAX_PATH];
    UINT len = GetSystemDirectoryW(dir, MAX_PATH);
    if (len == 0 || len >= MAX_PATH - 12) {
        context = INVALID_HANDLE_VALUE;
        return context;
    }
    wcscpy_s(shell32, MAX_PATH, dir);
    wcscat_s(shell32, MAX_PATH, L"\\shell32.dll");
    ACTCTXW act;
    memset(&act, 0, sizeof act);
    act.cbSize = sizeof act;
    act.dwFlags = ACTCTX_FLAG_RESOURCE_NAME_VALID | ACTCTX_FLAG_ASSEMBLY_DIRECTORY_VALID;
    act.lpSource = shell32;
    act.lpAssemblyDirectory = dir;
    act.lpResourceName = MAKEINTRESOURCEW(124);
    context = CreateActCtxW(&act);
    return context;
}

static int plain_message_box(HWND owner, int kind, const wchar_t* title, const wchar_t* text, int buttons) {
    UINT flags = MB_OK;
    switch (buttons) {
    case NG_MESSAGE_BUTTONS_OK_CANCEL: flags = MB_OKCANCEL; break;
    case NG_MESSAGE_BUTTONS_YES_NO: flags = MB_YESNO; break;
    case NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL: flags = MB_YESNOCANCEL; break;
    default: break;
    }
    flags |= kind == NG_MESSAGE_ERROR ? MB_ICONERROR
        : kind == NG_MESSAGE_WARNING ? MB_ICONWARNING : MB_ICONINFORMATION;
    if (owner) ng_windows_modal_begin(owner);
    int button = MessageBoxW(owner, text, title, flags);
    if (owner) ng_windows_modal_end(owner);
    if (button == 0) return NG_ERROR_PLATFORM_SPECIFIC;
    if (buttons == NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL) {
        if (button == IDYES) return NG_CHOICE_SAVE;
        if (button == IDNO) return NG_CHOICE_DISCARD;
    }
    return choice_of(button, buttons);
}

int ng_windows_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons) {
    HWND owner = (HWND)window;
    wchar_t* wide_title = ng_windows_utf8_to_wide(title);
    wchar_t* wide_text = ng_windows_utf8_to_wide(text);
    TASKDIALOGCONFIG config;
    memset(&config, 0, sizeof config);
    config.cbSize = sizeof config;
    config.hwndParent = owner;
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION;
    config.pszWindowTitle = wide_title;
    config.pszContent = wide_text;
    config.pszMainIcon = icon_of(kind);
    config.dwCommonButtons = common_buttons_of(buttons);
    if (buttons == NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL) {
        config.pButtons = save_discard_buttons;
        config.cButtons = (UINT)(sizeof save_discard_buttons / sizeof save_discard_buttons[0]);
    }

    int result;
    HANDLE v6 = ng_windows_common_controls_v6();
    ULONG_PTR cookie = 0;
    BOOL activated = v6 != INVALID_HANDLE_VALUE && ActivateActCtx(v6, &cookie);
    HMODULE comctl = LoadLibraryW(L"comctl32.dll");
    TaskDialogIndirectFn task_dialog = comctl
        ? (TaskDialogIndirectFn)GetProcAddress(comctl, "TaskDialogIndirect")
        : NULL;
    if (task_dialog) {
        int button = 0;
        if (owner) ng_windows_modal_begin(owner);
        HRESULT shown = task_dialog(&config, &button, NULL, NULL);
        if (owner) ng_windows_modal_end(owner);
        result = FAILED(shown) ? NG_ERROR_PLATFORM_SPECIFIC : choice_of(button, buttons);
    } else {
        result = plain_message_box(owner, kind, wide_title, wide_text, buttons);
    }
    if (comctl) FreeLibrary(comctl);
    if (activated) DeactivateActCtx(0, cookie);
    free(wide_title);
    free(wide_text);
    return result;
}

static COLORREF custom_colors[16];

static UINT_PTR CALLBACK color_hook(HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) {
    (void)wparam;
    if (message == WM_INITDIALOG) {
        const CHOOSECOLORW* cc = (const CHOOSECOLORW*)lparam;
        const wchar_t* title = (const wchar_t*)cc->lCustData;
        if (title && *title) SetWindowTextW(dialog, title);
    }
    return 0;
}

int ng_windows_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out) {
    (void)alpha;
    HWND owner = (HWND)window;
    wchar_t* wide_title = title && *title ? ng_windows_utf8_to_wide(title) : NULL;
    CHOOSECOLORW cc;
    memset(&cc, 0, sizeof cc);
    cc.lStructSize = sizeof cc;
    cc.hwndOwner = owner;
    cc.rgbResult = RGB((argb_in >> 16) & 0xFF, (argb_in >> 8) & 0xFF, argb_in & 0xFF);
    cc.lpCustColors = custom_colors;
    cc.Flags = CC_RGBINIT | CC_FULLOPEN;
    if (wide_title) {
        cc.Flags |= CC_ENABLEHOOK;
        cc.lpfnHook = color_hook;
        cc.lCustData = (LPARAM)wide_title;
    }

    if (owner) ng_windows_modal_begin(owner);
    BOOL picked = ChooseColorW(&cc);
    if (owner) ng_windows_modal_end(owner);
    free(wide_title);
    if (!picked) return CommDlgExtendedError() ? NG_ERROR_PLATFORM_SPECIFIC : 0;

    unsigned int r = GetRValue(cc.rgbResult);
    unsigned int g = GetGValue(cc.rgbResult);
    unsigned int b = GetBValue(cc.rgbResult);
    *argb_out = (argb_in & 0xFF000000u) | (r << 16) | (g << 8) | b;
    return 1;
}
