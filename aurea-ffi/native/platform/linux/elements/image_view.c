#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>
#include <gdk-pixbuf/gdk-pixbuf.h>

#define IMAGE_SCALING_NONE 0
#define IMAGE_SCALING_ASPECT_FIT 1
#define IMAGE_SCALING_ASPECT_FILL 2
#define IMAGE_SCALING_FILL 3

#define PIXBUF_KEY "aurea-pixbuf"
#define SCALING_KEY "aurea-scaling"

static gboolean on_draw(GtkWidget* widget, cairo_t* cr, gpointer unused) {
    (void)unused;
    GdkPixbuf* pixbuf = g_object_get_data(G_OBJECT(widget), PIXBUF_KEY);
    if (!pixbuf) return FALSE;
    int mode = GPOINTER_TO_INT(g_object_get_data(G_OBJECT(widget), SCALING_KEY));
    double aw = gtk_widget_get_allocated_width(widget);
    double ah = gtk_widget_get_allocated_height(widget);
    double w = gdk_pixbuf_get_width(pixbuf);
    double h = gdk_pixbuf_get_height(pixbuf);
    if (w <= 0 || h <= 0) return FALSE;

    double sx = aw / w, sy = ah / h;
    if (mode == IMAGE_SCALING_NONE) {
        sx = sy = 1.0;
    } else if (mode == IMAGE_SCALING_ASPECT_FIT) {
        sx = sy = sx < sy ? sx : sy;
    } else if (mode == IMAGE_SCALING_ASPECT_FILL) {
        sx = sy = sx > sy ? sx : sy;
    }
    cairo_save(cr);
    cairo_rectangle(cr, 0, 0, aw, ah);
    cairo_clip(cr);
    cairo_translate(cr, (aw - w * sx) / 2.0, (ah - h * sy) / 2.0);
    cairo_scale(cr, sx, sy);
    gdk_cairo_set_source_pixbuf(cr, pixbuf, 0, 0);
    cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_GOOD);
    cairo_paint(cr);
    cairo_restore(cr);
    return FALSE;
}

NGHandle ng_linux_create_image_view(void) {
    GtkWidget* imageView = gtk_drawing_area_new();
    gtk_widget_set_size_request(imageView, 200, 150);
    g_object_set_data(G_OBJECT(imageView), SCALING_KEY, GINT_TO_POINTER(IMAGE_SCALING_ASPECT_FIT));
    g_signal_connect(imageView, "draw", G_CALLBACK(on_draw), NULL);
    gtk_widget_show(imageView);
    return (NGHandle)imageView;
}

static int set_pixbuf(NGHandle image_view, GdkPixbuf* pixbuf) {
    if (!pixbuf) return NG_ERROR_CREATION_FAILED;
    g_object_set_data_full(G_OBJECT(image_view), PIXBUF_KEY, pixbuf, g_object_unref);
    gtk_widget_queue_draw((GtkWidget*)image_view);
    return NG_SUCCESS;
}

int ng_linux_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    return set_pixbuf(image_view, gdk_pixbuf_new_from_file(path, NULL));
}

int ng_linux_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;

    GdkPixbufLoader* loader = gdk_pixbuf_loader_new();
    if (!loader) {
        return NG_ERROR_CREATION_FAILED;
    }
    GdkPixbuf* pixbuf = NULL;
    if (gdk_pixbuf_loader_write(loader, data, size, NULL) &&
        gdk_pixbuf_loader_close(loader, NULL)) {
        pixbuf = gdk_pixbuf_loader_get_pixbuf(loader);
        if (pixbuf) g_object_ref(pixbuf);
    } else {
        gdk_pixbuf_loader_close(loader, NULL);
    }
    g_object_unref(loader);
    return set_pixbuf(image_view, pixbuf);
}

void ng_linux_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    g_object_set_data(G_OBJECT(image_view), SCALING_KEY, GINT_TO_POINTER(scaling_mode));
    gtk_widget_queue_draw((GtkWidget*)image_view);
}

void ng_linux_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    gtk_widget_queue_draw((GtkWidget*)image_view);
}
