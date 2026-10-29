#include "../elements.h"
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
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
    int gpu_owned;
    int allocated_width;
    int allocated_height;
} CanvasData;

static void ng_linux_canvas_destroy(GtkWidget* widget, gpointer user_data) {
    (void)user_data;
    ng_invoke_canvas_destroyed((void*)widget);
}

static void ng_linux_canvas_size_allocate(GtkWidget* widget, GdkRectangle* allocation, gpointer user_data) {
    CanvasData* data = (CanvasData*)user_data;
    if (allocation->width == data->allocated_width && allocation->height == data->allocated_height) return;
    data->allocated_width = allocation->width;
    data->allocated_height = allocation->height;
    ng_invoke_canvas_resized((void*)widget);
}

static gboolean ng_linux_canvas_draw(GtkWidget* widget, cairo_t* cr, gpointer user_data) {
    CanvasData* data = (CanvasData*)user_data;
    if (data && data->gpu_owned) return TRUE;

    GtkAllocation allocation;
    gtk_widget_get_allocation(widget, &allocation);

    if (data && data->buffer && data->width > 0 && data->height > 0) {
        cairo_surface_t* surface = cairo_image_surface_create_for_data(
            (unsigned char*)data->buffer,
            CAIRO_FORMAT_ARGB32,
            (int)data->width,
            (int)data->height,
            (int)data->width * 4);

        if (cairo_surface_status(surface) == CAIRO_STATUS_SUCCESS) {
            cairo_save(cr);
            if (allocation.width == (int)data->width &&
                allocation.height == (int)data->height) {
                cairo_set_source_surface(cr, surface, 0, 0);
                cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_NEAREST);
                cairo_paint(cr);
            } else {
                double scale_x = (allocation.width  > 0) ? (double)allocation.width  / (double)data->width  : 1.0;
                double scale_y = (allocation.height > 0) ? (double)allocation.height / (double)data->height : 1.0;
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

NGHandle ng_linux_create_canvas(int width, int height) {
    GtkWidget* drawing_area = gtk_drawing_area_new();
    gtk_widget_set_size_request(drawing_area, width, height);
    gtk_widget_show(drawing_area);

    CanvasData* data = g_new0(CanvasData, 1);
    g_object_set_data_full(G_OBJECT(drawing_area), "aurea-canvas-data", data, g_free);
    g_signal_connect(G_OBJECT(drawing_area), "draw", G_CALLBACK(ng_linux_canvas_draw), data);
    g_signal_connect(G_OBJECT(drawing_area), "size-allocate", G_CALLBACK(ng_linux_canvas_size_allocate), data);
    g_signal_connect(G_OBJECT(drawing_area), "destroy", G_CALLBACK(ng_linux_canvas_destroy), NULL);
    
    return (NGHandle)drawing_area;
}

void ng_linux_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
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
    if (!canvas) return;
    GtkWidget* widget = (GtkWidget*)canvas;
    CanvasData* data = (CanvasData*)g_object_get_data(G_OBJECT(widget), "aurea-canvas-data");
    if (!data) return;
    data->gpu_owned = gpu_owned != 0;
    if (data->gpu_owned) {
        data->buffer = NULL;
        data->width = 0;
        data->height = 0;
    }
}

void ng_linux_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    if (!canvas) return;

    GtkWidget* widget = (GtkWidget*)canvas;
    CanvasData* data = (CanvasData*)g_object_get_data(G_OBJECT(widget), "aurea-canvas-data");
    if (!data) return;
    if (!buffer || width == 0 || height == 0) {
        data->buffer = NULL;
        data->width = 0;
        data->height = 0;
        return;
    }

    int resized = data->width != width || data->height != height;
    data->buffer = buffer;
    data->width = width;
    data->height = height;

    // damage is queued by the caller; only a size change repaints everything
    if (resized) gtk_widget_queue_draw(widget);
}

void ng_linux_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;
    GtkWidget* widget = (GtkWidget*)canvas;
    GtkAllocation allocation;
    gtk_widget_get_allocation(widget, &allocation);
    *width = (unsigned int)allocation.width;
    *height = (unsigned int)allocation.height;
}

NGHandle ng_linux_canvas_get_window(NGHandle canvas) {
    if (!canvas) return NULL;
    GtkWidget* widget = (GtkWidget*)canvas;
    GtkWidget* toplevel = gtk_widget_get_toplevel(widget);
    if (GTK_IS_WINDOW(toplevel)) {
        return (NGHandle)toplevel;
    }
    return NULL;
}

NGHandle ng_linux_canvas_get_native_handle(NGHandle canvas) {
    if (!canvas) return NULL;
    GtkWidget* widget = (GtkWidget*)canvas;
    GdkWindow* gdk_window = gtk_widget_get_window(widget);
    if (gdk_window) {
        return (NGHandle)gdk_window;
    }
    return NULL;
}

int ng_linux_canvas_get_xcb_handle(NGHandle canvas, uint32_t* xcb_window, void** xcb_connection) {
    if (!canvas || !xcb_window || !xcb_connection) return 0;
    GdkWindow* gdk_window = NULL;
    if (GDK_IS_WINDOW(canvas)) {
        gdk_window = (GdkWindow*)canvas;
    } else {
        GtkWidget* widget = (GtkWidget*)canvas;
        gdk_window = gtk_widget_get_window(widget);
    }
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
    GdkWindow* gdk_window = NULL;
    if (GDK_IS_WINDOW(canvas)) {
        gdk_window = (GdkWindow*)canvas;
    } else {
        GtkWidget* widget = (GtkWidget*)canvas;
        gdk_window = gtk_widget_get_window(widget);
    }
    if (!gdk_window) return 0;
#ifdef GDK_WINDOWING_WAYLAND
    if (GDK_IS_WAYLAND_WINDOW(gdk_window)) {
        GdkDisplay* gdk_display = gdk_window_get_display(gdk_window);
        if (!gdk_display) return 0;
        *surface = gdk_wayland_window_get_wl_surface(gdk_window);
        *display = gdk_wayland_display_get_wl_display(gdk_display);
        if (*surface == NULL || *display == NULL) {
            return 0;
        }
        return 1;
    }
#endif
    return 0;
}
