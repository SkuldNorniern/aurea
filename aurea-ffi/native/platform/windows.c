#include "windows.h"
#include "windows/utils.h"
#include "windows/window.h"
#include "windows/menu.h"
#include "windows/elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <windows.h>

static HANDLE g_frame_event = NULL;

void ng_windows_request_frame(void) {
    if (g_frame_event) SetEvent(g_frame_event);
}

int ng_windows_run(void) {
    g_frame_event = CreateEventA(NULL, FALSE, FALSE, NULL); // auto-reset

    for (;;) {
        DWORD result = MsgWaitForMultipleObjectsEx(
            1, &g_frame_event,
            INFINITE,
            QS_ALLINPUT,
            MWMO_ALERTABLE | MWMO_INPUTAVAILABLE);

        if (result == WAIT_OBJECT_0) {
            ng_process_frames();
        } else if (result == WAIT_OBJECT_0 + 1) {
            MSG msg;
            while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
                if (msg.message == WM_QUIT) goto done;
                ng_process_frames();
                TranslateMessage(&msg);
                DispatchMessageA(&msg);
            }
        } else {
            break;
        }
    }
done:
    CloseHandle(g_frame_event);
    g_frame_event = NULL;
    return NG_SUCCESS;
}

int ng_windows_poll_events(void) {
    MSG msg;
    while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }
    return NG_SUCCESS;
}
