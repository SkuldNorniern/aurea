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

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_LINUX_UTILS_H

