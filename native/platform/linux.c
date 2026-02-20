#include "linux.h"
#include "linux/utils.h"
#include "linux/window.h"
#include "linux/menu.h"
#include "linux/elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

static gboolean process_frames_idle(gpointer user_data) {
    (void)user_data;
    ng_process_frames();
    return G_SOURCE_CONTINUE;
}

int ng_linux_run(void) {
    g_idle_add(process_frames_idle, NULL);
    gtk_main();
    return NG_SUCCESS;
}

int ng_linux_poll_events(void) {
    while (g_main_context_iteration(NULL, FALSE));
    return NG_SUCCESS;
}
