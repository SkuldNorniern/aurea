#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif
#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#endif

typedef struct {
    const unsigned char* buffer;
    unsigned int width;
    unsigned int height;
} CanvasData;

static gboolean ng_linux_canvas_draw(GtkWidget* widget, cairo_t* cr, gpointer user_data) {
    CanvasData* data = (CanvasData*)user_data;
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
            double scale_x = 1.0;
            double scale_y = 1.0;
            if (allocation.width > 0 && allocation.height > 0) {
                scale_x = (double)allocation.width / (double)data->width;
                scale_y = (double)allocation.height / (double)data->height;
            }

            cairo_save(cr);
            cairo_scale(cr, scale_x, scale_y);
            cairo_set_source_surface(cr, surface, 0, 0);
            cairo_paint(cr);
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
    
    return (NGHandle)drawing_area;
}

void ng_linux_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

void ng_linux_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    GtkWidget* widget = (GtkWidget*)canvas;
    cairo_region_t* region = cairo_region_create_rectangle((cairo_rectangle_int_t*)&(cairo_rectangle_int_t){
        .x = (int)x,
        .y = (int)y,
        .width = (int)width,
        .height = (int)height
    });
    gtk_widget_queue_draw_region(widget, region);
    cairo_region_destroy(region);
}

void ng_linux_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    (void)size;
    if (!canvas || !buffer || width == 0 || height == 0) return;

    GtkWidget* widget = (GtkWidget*)canvas;
    CanvasData* data = (CanvasData*)g_object_get_data(G_OBJECT(widget), "aurea-canvas-data");
    if (!data) return;

    data->buffer = buffer;
    data->width = width;
    data->height = height;

    gtk_widget_queue_draw(widget);
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
    GtkWindow* window = GTK_WINDOW(gtk_widget_get_toplevel(widget));
    if (GTK_IS_WINDOW(window)) {
        return (NGHandle)window;
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
#ifdef GDK_WINDOWING_X11
    if (GDK_IS_X11_WINDOW(gdk_window)) {
        GdkDisplay* display = gdk_window_get_display(gdk_window);
        if (!display) return 0;
        *xcb_window = (uint32_t)gdk_x11_window_get_xid(gdk_window);
        *xcb_connection = gdk_x11_display_get_xcb_connection(display);
        if (*xcb_window == 0 || *xcb_connection == NULL) {
            return 0;
        }
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
