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

/* Asking for a frame and running one are apart. A request only sets the
   event; the loop runs at most one frame per display refresh, so a ticker
   that asks again at the end of every frame gets the refresh rate instead of
   every frame the CPU can make, and whatever was asked for in between lands
   in the same frame. */
static HANDLE g_frame_event = NULL;
static HANDLE g_frame_timer = NULL;
static BOOL g_timer_armed = FALSE;
static BOOL g_in_frame = FALSE;
static LONGLONG g_qpc_freq = 0;
static LONGLONG g_frame_interval = 0;
static LONGLONG g_next_frame = 0;

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
    /* 0 and 1 both mean the hardware default. */
    if (EnumDisplaySettingsW(NULL, ENUM_CURRENT_SETTINGS, &mode) && mode.dmDisplayFrequency > 1) {
        return mode.dmDisplayFrequency;
    }
    return 60;
}

void ng_windows_display_changed(void) {
    if (g_qpc_freq > 0) g_frame_interval = g_qpc_freq / ng_windows_refresh_hz();
}

unsigned int ng_windows_frame_interval_ms(void) {
    if (g_qpc_freq <= 0 || g_frame_interval <= 0) return 16;
    LONGLONG ms = g_frame_interval * 1000 / g_qpc_freq;
    return ms > 0 ? (unsigned int)ms : 1;
}

static void ng_windows_run_frame(void) {
    /* A modal loop opened inside a frame, a message box say, pumps messages,
       and one of them can be the modal frame timer. */
    if (g_in_frame) return;
    LONGLONG now = ng_windows_now();
    g_next_frame += g_frame_interval;
    if (g_next_frame <= now) g_next_frame = now + g_frame_interval;
    g_in_frame = TRUE;
    ng_process_frames();
    g_in_frame = FALSE;
}

/* A frame was asked for: run it now if a refresh has passed since the last
   one, otherwise wake when it has. */
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

/* Moving, sizing and menus run their own message loop inside
   DispatchMessage, so ng_windows_run does not get to wait on anything until
   they end. The window proc calls this from a timer meanwhile. */
void ng_windows_modal_tick(void) {
    if (!g_frame_event) return;
    HANDLE handles[2] = { g_frame_event, g_frame_timer };
    DWORD count = g_frame_timer ? 2 : 1;
    DWORD result = WaitForMultipleObjects(count, handles, FALSE, 0);
    if (result == WAIT_OBJECT_0 + 1) g_timer_armed = FALSE;
    if (result == WAIT_OBJECT_0 || result == WAIT_OBJECT_0 + 1) {
        ng_windows_run_frame();
    }
}

int ng_windows_run(void) {
    LARGE_INTEGER freq;
    QueryPerformanceFrequency(&freq);
    g_qpc_freq = freq.QuadPart;
    ng_windows_display_changed();
    g_next_frame = 0;
    g_timer_armed = FALSE;

    g_frame_event = CreateEventA(NULL, FALSE, FALSE, NULL); // auto-reset
    /* The plain timer ticks at the system timer resolution, 15.6 ms unless
       something raised it, which is a frame at 60 Hz every other refresh. */
    g_frame_timer = CreateWaitableTimerExW(NULL, NULL, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS);
    if (!g_frame_timer) g_frame_timer = CreateWaitableTimerW(NULL, FALSE, NULL);
    /* Flush any frames scheduled before the loop started (e.g. set_draw_callback
       called before window.run()).  notify_platform() was a no-op then because
       g_frame_event was NULL; signal it now so the first iteration fires. */
    SetEvent(g_frame_event);

    HANDLE handles[2] = { g_frame_event, g_frame_timer };
    DWORD count = g_frame_timer ? 2 : 1;
    for (;;) {
        DWORD result = MsgWaitForMultipleObjectsEx(
            count, handles,
            INFINITE,
            QS_ALLINPUT,
            MWMO_ALERTABLE | MWMO_INPUTAVAILABLE);

        BOOL wanted = FALSE;
        if (result == WAIT_OBJECT_0) {
            wanted = TRUE;
        } else if (result == WAIT_OBJECT_0 + 1 && count == 2) {
            g_timer_armed = FALSE;
            wanted = TRUE;
        } else if (result != WAIT_OBJECT_0 + count && result != WAIT_IO_COMPLETION) {
            break;
        }

        /* Every waiting message first, frame after. Input handled here only
           queues events, so a burst of mouse moves collapses into one before
           the frame reads it, and a paint is never left behind a frame. */
        MSG msg;
        while (PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) {
            if (msg.message == WM_QUIT) goto done;
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if (wanted) ng_windows_frame_wanted();
    }
done:
    if (g_frame_timer) {
        CancelWaitableTimer(g_frame_timer);
        CloseHandle(g_frame_timer);
        g_frame_timer = NULL;
    }
    CloseHandle(g_frame_event);
    g_frame_event = NULL;
    return NG_SUCCESS;
}

int ng_windows_poll_events(void) {
    MSG msg;
    /* Wide, to match the window class. PeekMessageA on a wide window hands
       back WM_CHAR converted down to the ANSI codepage, which is exactly the
       loss the wide class exists to avoid. */
    while (PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return NG_SUCCESS;
}
