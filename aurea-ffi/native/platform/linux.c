#include "linux.h"
#include "linux/utils.h"
#include "linux/window.h"
#include "linux/menu.h"
#include "linux/elements.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>

static guint g_frame_source_id = 0;
/* At most one frame per refresh. A ticker asks for the next frame at the
   end of each one, and an idle source answered at once, so it ran as many
   frames as the CPU could make. */
static gint64 g_next_frame_us = 0;
static gint64 g_frame_interval_us = 16667;

/* The fastest monitor's refresh. Read here on the UI thread because a
   frame can be asked for from any thread, and GDK is not for those. */
static void ng_linux_update_frame_interval(void) {
    GdkDisplay* display = gdk_display_get_default();
    int best_mhz = 0;
    if (display) {
        int count = gdk_display_get_n_monitors(display);
        for (int i = 0; i < count; i++) {
            GdkMonitor* monitor = gdk_display_get_monitor(display, i);
            int mhz = monitor ? gdk_monitor_get_refresh_rate(monitor) : 0;
            if (mhz > best_mhz) best_mhz = mhz;
        }
    }
    if (best_mhz <= 0) best_mhz = 60000;
    g_frame_interval_us = (gint64)1000000000 / best_mhz;
}

static gboolean process_frames_once(gpointer user_data) {
    (void)user_data;
    g_frame_source_id = 0;
    ng_linux_update_frame_interval();
    /* From the last deadline rather than from now, so waking a little late
       does not push every later frame back with it. */
    gint64 now = g_get_monotonic_time();
    g_next_frame_us += g_frame_interval_us;
    if (g_next_frame_us <= now) g_next_frame_us = now + g_frame_interval_us;
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

/* Set while a request is on its way to the UI thread, so a burst of them
   from another thread posts one. */
static gint g_request_posted = 0;

/* Runs only on the thread holding the main context, the UI thread once
   gtk_main runs, so the frame source and the deadline are never touched by
   two threads at once. */
static gboolean ng_linux_schedule_frame(gpointer user_data) {
    (void)user_data;
    /* Cleared first, so a request that comes in from here on posts again. */
    g_atomic_int_set(&g_request_posted, 0);
    if (g_frame_source_id != 0) return G_SOURCE_REMOVE;
    gint64 wait_us = g_next_frame_us - g_get_monotonic_time();
    /* Default priority, the same as input. At idle priority a frame only ran
       once nothing else was ready, so steady input could hold it off
       indefinitely. Pacing already keeps it to one per refresh. */
    guint delay_ms = wait_us > 0 ? (guint)((wait_us + 999) / 1000) : 0;
    g_frame_source_id = g_timeout_add_full(G_PRIORITY_DEFAULT, delay_ms,
        process_frames_once, NULL, NULL);
    return G_SOURCE_REMOVE;
}

/* Any thread may ask for a frame; the scheduler promises that. On the UI
   thread this runs the request at once, from elsewhere it is handed over. */
void ng_linux_request_frame(void) {
    if (g_atomic_int_compare_and_exchange(&g_request_posted, 0, 1)) {
        g_main_context_invoke(NULL, ng_linux_schedule_frame, NULL);
    }
}
