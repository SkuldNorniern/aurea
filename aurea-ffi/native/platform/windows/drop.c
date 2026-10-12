
#define COBJMACROS
#include "drop.h"
#include "utils.h"
#include "elements/common.h"
#include "common/rust_callbacks.h"
#include "common/types.h"
#include <windows.h>
#include <ole2.h>
#include <shellapi.h>
#include <stdlib.h>

typedef struct {
    IDropTarget target;
    LONG refs;
    HWND hwnd;
    char* paths;
} DropTarget;

static char* paths_of(IDataObject* data) {
    FORMATETC format = {CF_HDROP, NULL, DVASPECT_CONTENT, -1, TYMED_HGLOBAL};
    STGMEDIUM medium;
    if (FAILED(IDataObject_GetData(data, &format, &medium))) return NULL;
    HDROP drop = (HDROP)GlobalLock(medium.hGlobal);
    char* out = NULL;
    size_t len = 0;
    if (drop) {
        UINT count = DragQueryFileW(drop, 0xFFFFFFFF, NULL, 0);
        for (UINT i = 0; i < count; i++) {
            UINT chars = DragQueryFileW(drop, i, NULL, 0) + 1;
            wchar_t* wide = (wchar_t*)malloc(chars * sizeof(wchar_t));
            if (!wide) continue;
            DragQueryFileW(drop, i, wide, chars);
            int n = WideCharToMultiByte(CP_UTF8, 0, wide, -1, NULL, 0, NULL, NULL);
            char* grown = (char*)realloc(out, len + (size_t)n + 1);
            if (grown) {
                out = grown;
                if (len) out[len++] = '\n';
                WideCharToMultiByte(CP_UTF8, 0, wide, -1, out + len, n, NULL, NULL);
                len += (size_t)n - 1;
            }
            free(wide);
        }
        GlobalUnlock(medium.hGlobal);
    }
    ReleaseStgMedium(&medium);
    return out;
}

static void content_point(HWND hwnd, POINTL pt, double* x, double* y) {
    POINT p = {pt.x, pt.y};
    ScreenToClient(hwnd, &p);
    *x = ng_windows_logical(hwnd, p.x);
    *y = ng_windows_logical(hwnd, p.y);
}

static void report(DropTarget* self, unsigned int kind, POINTL pt) {
    double x = 0.0, y = 0.0;
    content_point(self->hwnd, pt, &x, &y);
    ng_invoke_file_drop((void*)self->hwnd, kind, x, y, self->paths ? self->paths : "");
}

static HRESULT STDMETHODCALLTYPE query_interface(IDropTarget* t, REFIID riid, void** out) {
    if (IsEqualIID(riid, &IID_IUnknown) || IsEqualIID(riid, &IID_IDropTarget)) {
        *out = t;
        IDropTarget_AddRef(t);
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE add_ref(IDropTarget* t) {
    return (ULONG)InterlockedIncrement(&((DropTarget*)t)->refs);
}

static ULONG STDMETHODCALLTYPE release(IDropTarget* t) {
    DropTarget* self = (DropTarget*)t;
    LONG refs = InterlockedDecrement(&self->refs);
    if (refs == 0) {
        free(self->paths);
        free(self);
    }
    return (ULONG)refs;
}

static HRESULT STDMETHODCALLTYPE drag_enter(IDropTarget* t, IDataObject* data, DWORD keys, POINTL pt, DWORD* effect) {
    (void)keys;
    DropTarget* self = (DropTarget*)t;
    free(self->paths);
    self->paths = paths_of(data);
    *effect = self->paths ? DROPEFFECT_COPY : DROPEFFECT_NONE;
    if (self->paths) report(self, NG_DROP_HOVER, pt);
    return S_OK;
}

static HRESULT STDMETHODCALLTYPE drag_over(IDropTarget* t, DWORD keys, POINTL pt, DWORD* effect) {
    (void)keys;
    DropTarget* self = (DropTarget*)t;
    *effect = self->paths ? DROPEFFECT_COPY : DROPEFFECT_NONE;
    if (self->paths) report(self, NG_DROP_HOVER, pt);
    return S_OK;
}

static HRESULT STDMETHODCALLTYPE drag_leave(IDropTarget* t) {
    DropTarget* self = (DropTarget*)t;
    if (self->paths) {
        POINTL none = {0, 0};
        report(self, NG_DROP_LEAVE, none);
    }
    free(self->paths);
    self->paths = NULL;
    return S_OK;
}

static HRESULT STDMETHODCALLTYPE drop(IDropTarget* t, IDataObject* data, DWORD keys, POINTL pt, DWORD* effect) {
    (void)keys;
    DropTarget* self = (DropTarget*)t;
    free(self->paths);
    self->paths = paths_of(data);
    *effect = self->paths ? DROPEFFECT_COPY : DROPEFFECT_NONE;
    if (self->paths) report(self, NG_DROP_DROP, pt);
    free(self->paths);
    self->paths = NULL;
    return S_OK;
}

static IDropTargetVtbl g_vtable = {
    query_interface, add_ref, release, drag_enter, drag_over, drag_leave, drop,
};

void ng_windows_accept_drops(HWND hwnd) {
    static int ole = 0;
    if (!ole) {
        HRESULT hr = OleInitialize(NULL);
        ole = SUCCEEDED(hr) ? 1 : -1;
    }
    if (ole < 0) return;
    DropTarget* self = (DropTarget*)calloc(1, sizeof(DropTarget));
    if (!self) return;
    self->target.lpVtbl = &g_vtable;
    self->refs = 1;
    self->hwnd = hwnd;
    if (FAILED(RegisterDragDrop(hwnd, &self->target))) {
        free(self);
        return;
    }
    IDropTarget_Release(&self->target);
}

void ng_windows_refuse_drops(HWND hwnd) {
    RevokeDragDrop(hwnd);
}
