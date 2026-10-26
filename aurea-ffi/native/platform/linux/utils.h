#ifndef NATIVE_GUI_LINUX_UTILS_H
#define NATIVE_GUI_LINUX_UTILS_H

#include <gtk/gtk.h>

#ifdef __cplusplus
extern "C" {
#endif

int ng_linux_init(void);
void ng_linux_cleanup(void);
int ng_linux_run(void);
int ng_linux_poll_events(void);
int ng_linux_wait_events(int timeout_ms);
void ng_linux_request_frame(void);

int ng_linux_reduced_motion(void);
int ng_linux_prefers_dark(void);
void ng_linux_beep(void);

#ifdef __cplusplus
}
#endif

#endif

