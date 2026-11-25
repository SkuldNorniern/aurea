/* What screen readers are told about controls, through MSAA property annotations. */

#include "common.h"
#include "../utils.h"
#include "common/errors.h"
#include <initguid.h>
#include <oleacc.h>
#include <stdlib.h>

/* Per thread: the service lives in the apartment of the thread that made it. */
static IAccPropServices* acc_props(void) {
    static AUREA_THREAD_LOCAL IAccPropServices* props;
    static AUREA_THREAD_LOCAL int tried;
    if (!tried) {
        tried = 1;
        HRESULT init = CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
        if (SUCCEEDED(init) || init == RPC_E_CHANGED_MODE) {
            CoCreateInstance(&CLSID_AccPropServices, NULL, CLSCTX_INPROC_SERVER,
                             &IID_IAccPropServices, (void**)&props);
        }
    }
    return props;
}

#define VALUE_SERVER_PROP "AureaAccValue"
#define STATE_SERVER_PROP "AureaAccState"
#define NAME_SERVER_PROP "AureaAccName"
#define ACCESSIBLE_SUBCLASS_ID 0xACC

/* Annotations outlive their window; a later window given the same handle would inherit them. */
static void forget_accessible(HWND hwnd) {
    RemovePropA(hwnd, VALUE_SERVER_PROP);
    RemovePropA(hwnd, STATE_SERVER_PROP);
    RemovePropA(hwnd, NAME_SERVER_PROP);
    IAccPropServices* props = acc_props();
    if (!props) return;
    MSAAPROPID all[] = {PROPID_ACC_NAME, PROPID_ACC_ROLE, PROPID_ACC_VALUE, PROPID_ACC_STATE};
    props->lpVtbl->ClearHwndProps(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, all, 4);
}

static LRESULT CALLBACK accessible_proc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam,
                                        UINT_PTR id, DWORD_PTR data) {
    (void)data;
    if (msg == WM_NCDESTROY) {
        forget_accessible(hwnd);
        ng_windows_remove_subclass(hwnd, accessible_proc, id);
    }
    return ng_windows_def_subclass(hwnd, msg, wParam, lParam);
}

static IAccPropServices* annotate(HWND hwnd) {
    IAccPropServices* props = acc_props();
    if (props) ng_windows_set_subclass(hwnd, accessible_proc, ACCESSIBLE_SUBCLASS_ID, 0);
    return props;
}

void ng_windows_set_role(HWND hwnd, long role) {
    IAccPropServices* props = annotate(hwnd);
    if (!props) return;
    if (!role) {
        MSAAPROPID prop = PROPID_ACC_ROLE;
        props->lpVtbl->ClearHwndProps(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, &prop, 1);
        return;
    }
    VARIANT value;
    VariantInit(&value);
    value.vt = VT_I4;
    value.lVal = role;
    props->lpVtbl->SetHwndProp(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, PROPID_ACC_ROLE, value);
}

/* Layout containers are STATIC windows; without this a screen reader stops on each as empty text. */
void ng_windows_mark_container(HWND hwnd) {
    ng_windows_set_role(hwnd, ROLE_SYSTEM_PANE);
}

static HRESULT STDMETHODCALLTYPE server_query(IAccPropServer* self, REFIID iid, void** out) {
    if (IsEqualIID(iid, &IID_IUnknown) || IsEqualIID(iid, &IID_IAccPropServer)) {
        *out = self;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE server_ref(IAccPropServer* self) {
    (void)self;
    return 1;
}

static HRESULT STDMETHODCALLTYPE server_get(IAccPropServer* self, const BYTE* id, DWORD id_len,
                                            MSAAPROPID prop, VARIANT* out, BOOL* has) {
    (void)self;
    VariantInit(out);
    *has = FALSE;
    IAccPropServices* props = acc_props();
    HWND hwnd = NULL;
    DWORD object = 0, child = 0;
    if (!props || FAILED(props->lpVtbl->DecomposeHwndIdentityString(props, id, id_len, &hwnd,
                                                                    &object, &child))) {
        return S_OK;
    }
    if (IsEqualGUID(&prop, &PROPID_ACC_VALUE) || IsEqualGUID(&prop, &PROPID_ACC_NAME)) {
        const char* key = IsEqualGUID(&prop, &PROPID_ACC_NAME) ? NAME_SERVER_PROP : VALUE_SERVER_PROP;
        int (*value)(HWND, wchar_t*, int) = (int (*)(HWND, wchar_t*, int))GetPropA(hwnd, key);
        wchar_t text[256];
        if (!value || !value(hwnd, text, 256)) return S_OK;
        out->vt = VT_BSTR;
        out->bstrVal = SysAllocString(text);
        *has = out->bstrVal != NULL;
    } else if (IsEqualGUID(&prop, &PROPID_ACC_STATE)) {
        long (*state)(HWND) = (long (*)(HWND))GetPropA(hwnd, STATE_SERVER_PROP);
        if (!state) return S_OK;
        out->vt = VT_I4;
        out->lVal = state(hwnd);
        *has = TRUE;
    }
    return S_OK;
}

static IAccPropServerVtbl g_server_vtbl = {server_query, server_ref, server_ref, server_get};
static IAccPropServer g_server = {&g_server_vtbl};

static void serve(HWND hwnd, const char* name, void* callback, MSAAPROPID prop) {
    IAccPropServices* props = annotate(hwnd);
    if (!props) return;
    SetPropA(hwnd, name, (HANDLE)callback);
    props->lpVtbl->SetHwndPropServer(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, &prop, 1,
                                     &g_server, ANNO_THIS);
}

void ng_windows_serve_value(HWND hwnd, int (*value)(HWND hwnd, wchar_t* out, int len)) {
    serve(hwnd, VALUE_SERVER_PROP, (void*)value, PROPID_ACC_VALUE);
}

void ng_windows_serve_state(HWND hwnd, long (*state)(HWND hwnd)) {
    serve(hwnd, STATE_SERVER_PROP, (void*)state, PROPID_ACC_STATE);
}

void ng_windows_serve_name(HWND hwnd, int (*name)(HWND hwnd, wchar_t* out, int len)) {
    serve(hwnd, NAME_SERVER_PROP, (void*)name, PROPID_ACC_NAME);
}

int ng_windows_element_set_accessible_name(NGHandle element, const char* name) {
    HWND hwnd = (HWND)element;
    if (!hwnd || !IsWindow(hwnd)) return NG_ERROR_INVALID_HANDLE;
    if (!name) return NG_ERROR_INVALID_PARAMETER;
    if (GetPropA(hwnd, SIDEBAR_DATA_PROP) || GetPropA(hwnd, TABLE_DATA_PROP)) {
        HWND list = GetWindow(hwnd, GW_CHILD);
        if (list) hwnd = list;
    }
    IAccPropServices* props = annotate(hwnd);
    if (!props) return NG_ERROR_PLATFORM_SPECIFIC;
    if (!*name) {
        MSAAPROPID prop = PROPID_ACC_NAME;
        props->lpVtbl->ClearHwndProps(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF, &prop, 1);
        return NG_SUCCESS;
    }
    wchar_t* wide = ng_windows_utf8_to_wide(name);
    if (!wide) return NG_ERROR_INVALID_PARAMETER;
    HRESULT set = props->lpVtbl->SetHwndPropStr(props, hwnd, (DWORD)OBJID_CLIENT, CHILDID_SELF,
                                                PROPID_ACC_NAME, wide);
    free(wide);
    return SUCCEEDED(set) ? NG_SUCCESS : NG_ERROR_PLATFORM_SPECIFIC;
}
