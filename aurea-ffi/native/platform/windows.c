#include "windows.h"
#include "windows/utils.h"
#include "windows/window.h"
#include "windows/menu.h"
#include "windows/elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>
#include <stdio.h>

#ifndef CREATE_WAITABLE_TIMER_HIGH_RESOLUTION
#define CREATE_WAITABLE_TIMER_HIGH_RESOLUTION 0x00000002
#endif

static HANDLE g_frame_event = NULL;
static HANDLE g_frame_timer = NULL;
static BOOL g_timer_armed = FALSE;
static BOOL g_frame_pending = FALSE;
static BOOL g_in_frame = FALSE;
static LONGLONG g_qpc_freq = 0;
static LONGLONG g_frame_interval = 0;
static LONGLONG g_next_frame = 0;

#define AUREA_MESSAGE_BATCH 64
#define AUREA_MESSAGE_BUDGET_US 2000

void ng_windows_request_frame(void) {
    if (g_frame_event) SetEvent(g_frame_event);
}

static LONGLONG ng_windows_now(void) {
    LARGE_INTEGER t;
    QueryPerformanceCounter(&t);
    return t.QuadPart;
}

static DWORD ng_windows_refresh_hz(void) {
    DWORD hz = ng_windows_window_refresh_hz();
    if (hz > 0) return hz;
    DEVMODEW mode;
    ZeroMemory(&mode, sizeof(mode));
    mode.dmSize = sizeof(mode);
    if (EnumDisplaySettingsW(NULL, ENUM_CURRENT_SETTINGS, &mode) && mode.dmDisplayFrequency > 1) {
        return mode.dmDisplayFrequency;
    }
    return 60;
}

void ng_windows_display_changed(void) {
    if (g_qpc_freq > 0) g_frame_interval = g_qpc_freq / ng_windows_refresh_hz();
}

static BOOL ng_windows_drain_messages(void) {
    if (g_qpc_freq <= 0) {
        LARGE_INTEGER freq;
        QueryPerformanceFrequency(&freq);
        g_qpc_freq = freq.QuadPart;
    }
    LONGLONG start = ng_windows_now();
    LONGLONG budget = g_qpc_freq * AUREA_MESSAGE_BUDGET_US / 1000000;
    MSG msg;
    for (int handled = 0; handled < AUREA_MESSAGE_BATCH; handled++) {
        if (!PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) break;
        if (msg.message == WM_QUIT) return FALSE;
        if (msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN) {
            HWND root = GetAncestor(msg.hwnd, GA_ROOT);
            if (root && root != msg.hwnd && ng_windows_take_shortcut(root, msg.wParam)) continue;
            if (msg.message == WM_KEYDOWN && msg.wParam == VK_TAB &&
                ng_windows_tab_focus(root, msg.hwnd, GetKeyState(VK_SHIFT) < 0,
                                     GetKeyState(VK_CONTROL) < 0)) {
                continue;
            }
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
        if (ng_windows_now() - start >= budget) break;
    }
    return TRUE;
}

static void ng_windows_run_frame(void) {
    if (g_in_frame) {
        g_frame_pending = TRUE;
        return;
    }
    LONGLONG now = ng_windows_now();
    g_next_frame += g_frame_interval;
    if (g_next_frame <= now) g_next_frame = now + g_frame_interval;
    g_in_frame = TRUE;
    ng_process_frames();
    ng_windows_present();
    g_in_frame = FALSE;
}

static void ng_windows_frame_wanted(void) {
    LONGLONG now = ng_windows_now();
    if (now >= g_next_frame || !g_frame_timer) {
        ng_windows_run_frame();
        return;
    }
    if (!g_timer_armed) {
        LARGE_INTEGER due;
        due.QuadPart = -((g_next_frame - now) * 10000000 / g_qpc_freq);
        if (due.QuadPart == 0) due.QuadPart = -1;
        if (SetWaitableTimer(g_frame_timer, &due, 0, NULL, NULL, FALSE)) {
            g_timer_armed = TRUE;
        } else {
            ng_windows_run_frame();
        }
    }
}

void ng_windows_modal_begin(HWND hwnd) {
    SetTimer(hwnd, AUREA_FRAME_TIMER_ID, USER_TIMER_MINIMUM, NULL);
}

void ng_windows_modal_end(HWND hwnd) {
    KillTimer(hwnd, AUREA_FRAME_TIMER_ID);
}

void ng_windows_modal_tick(void) {
    if (!g_frame_event) return;
    HANDLE handles[2] = { g_frame_event, g_frame_timer };
    DWORD count = g_frame_timer ? 2 : 1;
    DWORD result = WaitForMultipleObjects(count, handles, FALSE, 0);
    if (result == WAIT_OBJECT_0 + 1) g_timer_armed = FALSE;
    if (result == WAIT_OBJECT_0 || result == WAIT_OBJECT_0 + 1) {
        g_frame_pending = TRUE;
    }
    if (g_frame_pending && ng_windows_now() >= g_next_frame) {
        g_frame_pending = FALSE;
        ng_windows_run_frame();
    }
}

static void ng_windows_loop_init(void) {
    if (g_frame_event) return;
    LARGE_INTEGER freq;
    QueryPerformanceFrequency(&freq);
    g_qpc_freq = freq.QuadPart;
    ng_windows_display_changed();
    g_next_frame = 0;
    g_timer_armed = FALSE;
    g_frame_pending = FALSE;

    g_frame_event = CreateEventA(NULL, FALSE, FALSE, NULL);
    typedef HANDLE (WINAPI *TimerExFunc)(LPSECURITY_ATTRIBUTES, LPCWSTR, DWORD, DWORD);
    TimerExFunc timer_ex = (TimerExFunc)ng_windows_api("kernel32.dll", "CreateWaitableTimerExW");
    g_frame_timer = timer_ex ? timer_ex(NULL, NULL, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS) : NULL;
    if (!g_frame_timer) g_frame_timer = CreateWaitableTimerW(NULL, FALSE, NULL);
    SetEvent(g_frame_event);
}

static void ng_windows_loop_cleanup(void) {
    if (g_frame_timer) {
        CancelWaitableTimer(g_frame_timer);
        CloseHandle(g_frame_timer);
        g_frame_timer = NULL;
    }
    if (g_frame_event) {
        CloseHandle(g_frame_event);
        g_frame_event = NULL;
    }
}

static BOOL ng_windows_turn(DWORD timeout_ms) {
    HANDLE handles[2] = { g_frame_event, g_frame_timer };
    DWORD count = g_frame_timer ? 2 : 1;
    DWORD result = MsgWaitForMultipleObjectsEx(
        count, handles,
        timeout_ms,
        QS_ALLINPUT,
        MWMO_ALERTABLE | MWMO_INPUTAVAILABLE);

    BOOL wanted = FALSE;
    if (result == WAIT_OBJECT_0) {
        wanted = TRUE;
    } else if (result == WAIT_OBJECT_0 + 1 && count == 2) {
        g_timer_armed = FALSE;
        wanted = TRUE;
    } else if (result == WAIT_TIMEOUT) {
        return TRUE;
    } else if (result != WAIT_OBJECT_0 + count && result != WAIT_IO_COMPLETION) {
        return FALSE;
    }

    if (!ng_windows_drain_messages()) return FALSE;

    if (!wanted) {
        DWORD due = WaitForMultipleObjects(count, handles, FALSE, 0);
        if (due == WAIT_OBJECT_0 + 1) g_timer_armed = FALSE;
        wanted = due == WAIT_OBJECT_0 || due == WAIT_OBJECT_0 + 1;
    }

    if (wanted || g_frame_pending) {
        g_frame_pending = FALSE;
        ng_windows_frame_wanted();
    }
    return TRUE;
}

int ng_windows_run(void) {
    ng_windows_loop_init();
    while (ng_windows_turn(INFINITE)) {
    }
    ng_windows_loop_cleanup();
    return NG_SUCCESS;
}

int ng_windows_wait_events(int timeout_ms) {
    ng_windows_loop_init();
    ng_windows_turn(timeout_ms < 0 ? INFINITE : (DWORD)timeout_ms);
    return NG_SUCCESS;
}

int ng_windows_poll_events(void) {
    ng_windows_drain_messages();
    return NG_SUCCESS;
}
