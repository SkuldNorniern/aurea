#include "../elements.h"
#include "../control.h"
#include "common/errors.h"
#include "common/image_fit.h"
#include <gtk/gtk.h>
#include <gdk-pixbuf/gdk-pixbuf.h>

typedef struct {
    NgControl control;
    GdkPixbuf* pixbuf;
    int scaling;
    gboolean laid_out;
} ImageState;

static const char image_kind = 0;

static void image_free(gpointer data) {
    ImageState* state = data;
    g_clear_object(&state->pixbuf);
    g_free(state);
}

static ImageState* state_of(NGHandle image_view) {
    return GTK_IS_WIDGET(image_view) ? ng_control_typed((GtkWidget*)image_view, &image_kind) : NULL;
}

static gboolean on_draw(GtkWidget* widget, cairo_t* cr, gpointer data) {
    ImageState* state = data;
    GdkPixbuf* pixbuf = state->pixbuf;
    if (!pixbuf) return FALSE;
    double aw = gtk_widget_get_allocated_width(widget);
    double ah = gtk_widget_get_allocated_height(widget);
    double w = gdk_pixbuf_get_width(pixbuf);
    double h = gdk_pixbuf_get_height(pixbuf);
    if (w <= 0 || h <= 0) return FALSE;

    NGImageRect r = ng_image_fit(state->scaling, aw, ah, w, h);
    cairo_save(cr);
    cairo_rectangle(cr, 0, 0, aw, ah);
    cairo_clip(cr);
    cairo_translate(cr, r.x, r.y);
    cairo_scale(cr, r.width / w, r.height / h);
    gdk_cairo_set_source_pixbuf(cr, pixbuf, 0, 0);
    cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_GOOD);
    cairo_paint(cr);
    cairo_restore(cr);
    return FALSE;
}

/* the starting size is a preference: after the first layout a box may give less */
static void on_size_allocate(GtkWidget* widget, GdkRectangle* allocation, gpointer data) {
    (void)allocation;
    ImageState* state = data;
    if (state->laid_out) return;
    state->laid_out = TRUE;
    gtk_widget_set_size_request(widget, -1, -1);
}

NGHandle ng_linux_create_image_view(void) {
    GtkWidget* image_view = gtk_drawing_area_new();
    gtk_widget_set_size_request(image_view, 200, 150);
    ImageState* state = ng_control_attach(image_view, &image_kind, sizeof(ImageState), image_free);
    state->scaling = NG_IMAGE_SCALING_ASPECT_FIT;
    g_signal_connect(image_view, "draw", G_CALLBACK(on_draw), state);
    g_signal_connect_after(image_view, "size-allocate", G_CALLBACK(on_size_allocate), state);
    gtk_widget_show(image_view);
    return (NGHandle)image_view;
}

/* Takes `pixbuf`, which may be NULL, and makes it upright; a failed load leaves nothing to show. */
static int set_pixbuf(NGHandle image_view, GdkPixbuf* pixbuf) {
    ImageState* state = state_of(image_view);
    if (!state) {
        g_clear_object(&pixbuf);
        return NG_ERROR_INVALID_HANDLE;
    }
    if (!pixbuf) return NG_ERROR_CREATION_FAILED;
    GdkPixbuf* upright = gdk_pixbuf_apply_embedded_orientation(pixbuf);
    g_object_unref(pixbuf);
    if (!upright) return NG_ERROR_CREATION_FAILED;
    g_clear_object(&state->pixbuf);
    state->pixbuf = upright;
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
    if (!loader) return NG_ERROR_CREATION_FAILED;
    GdkPixbuf* pixbuf = NULL;
    if (gdk_pixbuf_loader_write(loader, data, size, NULL) && gdk_pixbuf_loader_close(loader, NULL)) {
        pixbuf = gdk_pixbuf_loader_get_pixbuf(loader);
        if (pixbuf) g_object_ref(pixbuf);
    } else {
        gdk_pixbuf_loader_close(loader, NULL);
    }
    g_object_unref(loader);
    return set_pixbuf(image_view, pixbuf);
}

void ng_linux_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    ImageState* state = state_of(image_view);
    if (!state) return;
    state->scaling = scaling_mode;
    gtk_widget_queue_draw((GtkWidget*)image_view);
}

int ng_linux_image_view_get_size(NGHandle image_view, unsigned int* width, unsigned int* height) {
    if (!image_view || !width || !height) return NG_ERROR_INVALID_PARAMETER;
    ImageState* state = state_of(image_view);
    if (!state || !state->pixbuf) return NG_ERROR_INVALID_HANDLE;
    *width = (unsigned int)gdk_pixbuf_get_width(state->pixbuf);
    *height = (unsigned int)gdk_pixbuf_get_height(state->pixbuf);
    return NG_SUCCESS;
}

void ng_linux_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    gtk_widget_queue_draw((GtkWidget*)image_view);
}
