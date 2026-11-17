#include "gesture.h"
#include "window_state.h"
#include "common/input.h"
#include "common/rust_callbacks.h"

static void on_zoom_changed(GtkGestureZoom* gesture, gdouble scale, gpointer data) {
    (void)scale;
    gdouble delta = gtk_gesture_zoom_get_scale_delta(gesture);
    gdouble x = 0.0, y = 0.0;
    gtk_gesture_get_bounding_box_center(GTK_GESTURE(gesture), &x, &y);
    if (delta != 1.0) ng_invoke_gesture(data, NG_GESTURE_MAGNIFY, delta, x, y);
}

static void on_angle_changed(GtkGestureRotate* gesture, gdouble angle, gdouble angle_delta, gpointer data) {
    (void)angle;
    gdouble x = 0.0, y = 0.0;
    gtk_gesture_get_bounding_box_center(GTK_GESTURE(gesture), &x, &y);
    if (angle_delta != 0.0) ng_invoke_gesture(data, NG_GESTURE_ROTATE, angle_delta, x, y);
}

/* The gestures live as long as the window, which keeps them referenced. */
void ng_linux_connect_gestures(GtkWidget* window) {
    NgLinuxWindow* w = ng_linux_window_state(window);
    w->zoom = gtk_gesture_zoom_new(window);
    g_signal_connect(w->zoom, "scale-changed", G_CALLBACK(on_zoom_changed), window);
    w->rotate = gtk_gesture_rotate_new(window);
    g_signal_connect(w->rotate, "angle-changed", G_CALLBACK(on_angle_changed), window);
}
