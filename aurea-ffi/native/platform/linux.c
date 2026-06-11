#include "linux.h"
#include "linux/utils.h"
#include "linux/window.h"
#include "linux/menu.h"
#include "linux/elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

static guint g_frame_source_id = 0;

static gboolean process_frames_once(gpointer user_data) {
    (void)user_data;
    g_frame_source_id = 0;
    ng_process_frames();
    return G_SOURCE_REMOVE;
}

int ng_linux_run(void) {
    gtk_main();
    return NG_SUCCESS;
}

int ng_linux_poll_events(void) {
    int iterations = 0;
    while (iterations < 64 && g_main_context_iteration(NULL, FALSE)) {
        iterations++;
    }
    return NG_SUCCESS;
}

void ng_linux_request_frame(void) {
    if (g_frame_source_id == 0) {
        g_frame_source_id = g_idle_add(process_frames_once, NULL);
    }
}
