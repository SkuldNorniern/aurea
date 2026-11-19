#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <math.h>
#if defined(GDK_WINDOWING_X11) && defined(AUREA_HAVE_X11_XCB)
#include <gdk/gdkx.h>
#include <X11/Xlib-xcb.h>
#endif
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

typedef struct {
    NgControl control;
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
    gboolean gpu_owned;
    int allocated_width;
    int allocated_height;
    gboolean laid_out;
} CanvasState;

static void on_destroy(GtkWidget* widget, gpointer data) {
    (void)data;
    ng_invoke_canvas_destroyed((void*)widget);
}

/* After the default handler, so the new allocation is what the app reads back. */
static void on_size_allocate(GtkWidget* widget, GdkRectangle* allocation, gpointer data) {
    CanvasState* state = data;
    /* the size it is created with is a preference: after the first layout a box may give less */
    if (!state->laid_out) {
        state->laid_out = TRUE;
        gtk_widget_set_size_request(widget, -1, -1);
    }
    if (allocation->width == state->allocated_width && allocation->height == state->allocated_height) return;
    state->allocated_width = allocation->width;
    state->allocated_height = allocation->height;
    ng_invoke_canvas_resized((void*)widget);
}

static gboolean on_draw(GtkWidget* widget, cairo_t* cr, gpointer data) {
    CanvasState* state = data;
    if (state->gpu_owned) return TRUE;

    GtkAllocation allocation;
    gtk_widget_get_allocation(widget, &allocation);

    if (state->buffer && state->width > 0 && state->height > 0) {
        cairo_surface_t* surface = cairo_image_surface_create_for_data(
            (unsigned char*)state->buffer,
            CAIRO_FORMAT_ARGB32,
            (int)state->width,
            (int)state->height,
            (int)state->width * 4);

        if (cairo_surface_status(surface) == CAIRO_STATUS_SUCCESS) {
            cairo_save(cr);
            if (allocation.width == (int)state->width && allocation.height == (int)state->height) {
                cairo_set_source_surface(cr, surface, 0, 0);
                cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_NEAREST);
                cairo_paint(cr);
            } else {
                double scale_x = allocation.width > 0 ? (double)allocation.width / (double)state->width : 1.0;
                double scale_y = allocation.height > 0 ? (double)allocation.height / (double)state->height : 1.0;
                cairo_scale(cr, scale_x, scale_y);
                cairo_set_source_surface(cr, surface, 0, 0);
                cairo_paint(cr);
            }
            cairo_restore(cr);
        }

        cairo_surface_destroy(surface);
        return FALSE;
    }

    cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
    cairo_rectangle(cr, 0, 0, allocation.width, allocation.height);
    cairo_fill(cr);
    return FALSE;
}

static const char canvas_kind = 0;

static CanvasState* state_of(NGHandle canvas) {
    return GTK_IS_WIDGET(canvas) ? ng_control_typed((GtkWidget*)canvas, &canvas_kind) : NULL;
}

NGHandle ng_linux_create_canvas(int width, int height) {
    GtkWidget* drawing_area = gtk_drawing_area_new();
    gtk_widget_set_size_request(drawing_area, width, height);
    gtk_widget_show(drawing_area);

    CanvasState* state = ng_control_attach(drawing_area, &canvas_kind, sizeof(CanvasState), g_free);
    g_signal_connect(drawing_area, "draw", G_CALLBACK(on_draw), state);
    g_signal_connect_after(drawing_area, "size-allocate", G_CALLBACK(on_size_allocate), state);
    g_signal_connect(drawing_area, "destroy", G_CALLBACK(on_destroy), NULL);
    return (NGHandle)drawing_area;
}

void ng_linux_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas || !GTK_IS_WIDGET(canvas)) return;
    GtkWidget* widget = (GtkWidget*)canvas;
    if (!(width > 0.0f) || !(height > 0.0f)) return;
    if (!(x > -1e7f && y > -1e7f && x + width < 1e7f && y + height < 1e7f)) {
        gtk_widget_queue_draw(widget);
        return;
    }
    int scale = gtk_widget_get_scale_factor(widget);
    if (scale < 1) scale = 1;
    int left = (int)floor(x / scale);
    int top = (int)floor(y / scale);
    cairo_rectangle_int_t dirty = {
        .x = left,
        .y = top,
        .width = (int)ceil((x + width) / scale) - left,
        .height = (int)ceil((y + height) / scale) - top,
    };
    cairo_region_t* region = cairo_region_create_rectangle(&dirty);
    gtk_widget_queue_draw_region(widget, region);
    cairo_region_destroy(region);
}

void ng_linux_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned) {
    CanvasState* state = state_of(canvas);
    if (!state) return;
    state->gpu_owned = gpu_owned != 0;
    if (state->gpu_owned) {
        state->buffer = NULL;
        state->width = 0;
        state->height = 0;
    }
}

/* The caller keeps `buffer` alive until it is replaced; damage is queued by the caller. */
void ng_linux_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    CanvasState* state = state_of(canvas);
    if (!state) return;
    if (!buffer || width == 0 || height == 0) {
        state->buffer = NULL;
        state->width = 0;
        state->height = 0;
        return;
    }

    gboolean resized = state->width != width || state->height != height;
    state->buffer = buffer;
    state->width = width;
    state->height = height;
    if (resized) gtk_widget_queue_draw((GtkWidget*)canvas);
}

void ng_linux_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !GTK_IS_WIDGET(canvas) || !width || !height) return;
    GtkAllocation allocation;
    gtk_widget_get_allocation((GtkWidget*)canvas, &allocation);
    *width = (unsigned int)allocation.width;
    *height = (unsigned int)allocation.height;
}

NGHandle ng_linux_canvas_get_window(NGHandle canvas) {
    if (!canvas || !GTK_IS_WIDGET(canvas)) return NULL;
    GtkWidget* toplevel = gtk_widget_get_toplevel((GtkWidget*)canvas);
    return GTK_IS_WINDOW(toplevel) ? (NGHandle)toplevel : NULL;
}

NGHandle ng_linux_canvas_get_native_handle(NGHandle canvas) {
    if (!canvas || !GTK_IS_WIDGET(canvas)) return NULL;
    return (NGHandle)gtk_widget_get_window((GtkWidget*)canvas);
}

/* The GdkWindow of a canvas handle, which is either the widget or its GdkWindow. */
static GdkWindow* native_window_of(NGHandle canvas) {
    if (!canvas) return NULL;
    if (GDK_IS_WINDOW(canvas)) return (GdkWindow*)canvas;
    if (GTK_IS_WIDGET(canvas)) return gtk_widget_get_window((GtkWidget*)canvas);
    return NULL;
}

int ng_linux_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection) {
    if (!canvas || !xcb_window || !xcb_connection) return 0;
    GdkWindow* gdk_window = native_window_of(canvas);
    if (!gdk_window) return 0;
#if defined(GDK_WINDOWING_X11) && defined(AUREA_HAVE_X11_XCB)
    if (GDK_IS_X11_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display || !GDK_IS_X11_DISPLAY(gdk_display)) return 0;
        Display* xdisplay = gdk_x11_display_get_xdisplay(gdk_display);
        xcb_connection_t* connection = xdisplay ? XGetXCBConnection(xdisplay) : NULL;
        Window xid = gdk_x11_window_get_xid(gdk_window);
        if (!connection || xid == 0 || xid > UINT32_MAX) return 0;
        *xcb_window = (uint32_t)xid;
        *xcb_connection = connection;
        return 1;
    }
#endif
    return 0;
}

int ng_linux_canvas_get_wayland_handle(NGHandle canvas, void** surface, void** display) {
    if (!canvas || !surface || !display) return 0;
    GdkWindow* gdk_window = native_window_of(canvas);
    if (!gdk_window) return 0;
#ifdef GDK_WINDOWING_WAYLAND
    if (GDK_IS_WAYLAND_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display) return 0;
        *surface = gdk_wayland_window_get_wl_surface(gdk_window);
        *display = gdk_wayland_display_get_wl_display(gdk_display);
        if (*surface == NULL || *display == NULL) return 0;
        return 1;
    }
#endif
    return 0;
}
