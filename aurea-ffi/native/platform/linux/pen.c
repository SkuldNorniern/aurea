#include "pen.h"
#include "window_state.h"
#include "common/input.h"
#include "common/rust_callbacks.h"

enum { PEN_ERASER = 1, PEN_BARREL = 2 };

static double pen_age(GdkEvent* event) {
    guint32 stamp = gdk_event_get_time(event);
    if (stamp == GDK_CURRENT_TIME) return 0.0;
    guint32 now = (guint32)(g_get_monotonic_time() / 1000);
    return (double)(guint32)(now - stamp) / 1000.0;
}

static double pen_axis(GdkEvent* event, GdkAxisUse axis, double fallback) {
    double value = 0.0;
    return gdk_event_get_axis(event, axis, &value) ? value : fallback;
}

void ng_linux_pen_report(GtkWidget* widget, GdkEvent* event, double x, double y, guint state, unsigned int phase) {
    GdkDevice* device = gdk_event_get_source_device(event);
    if (!device) return;
    GdkInputSource source = gdk_device_get_source(device);
    if (source != GDK_SOURCE_PEN && source != GDK_SOURCE_ERASER) return;
    double pressure = pen_axis(event, GDK_AXIS_PRESSURE, phase == PEN_HOVER ? 0.0 : 1.0);
    float tilt_x = (float)(pen_axis(event, GDK_AXIS_XTILT, 0.0) * 90.0);
    float tilt_y = (float)(pen_axis(event, GDK_AXIS_YTILT, 0.0) * 90.0);
    float twist = (float)(pen_axis(event, GDK_AXIS_ROTATION, 0.0) * 360.0);
    unsigned int flags = 0;
    if (source == GDK_SOURCE_ERASER) flags |= PEN_ERASER;
    if (state & (GDK_BUTTON2_MASK | GDK_BUTTON3_MASK)) flags |= PEN_BARREL;
    ng_invoke_pen((void*)widget, x, y, (float)pressure, tilt_x, tilt_y, twist, phase, flags,
                  ng_linux_modifiers(state), pen_age(event));
}

unsigned int ng_linux_pen_bits(GdkInputSource source) {
    return source == GDK_SOURCE_PEN || source == GDK_SOURCE_ERASER ? NG_POINTER_PEN : 0;
}
