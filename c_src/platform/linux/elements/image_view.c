#include "../elements.h"
#include "../../common/errors.h"
#include <gtk/gtk.h>
#include <gdk-pixbuf/gdk-pixbuf.h>

// Scaling mode constants
#define IMAGE_SCALING_NONE 0
#define IMAGE_SCALING_ASPECT_FIT 1
#define IMAGE_SCALING_ASPECT_FILL 2
#define IMAGE_SCALING_FILL 3

NGHandle ng_linux_create_image_view(void) {
    GtkWidget* imageView = gtk_image_new();
    // Set maximum height to make image display smaller
    gtk_widget_set_size_request(imageView, -1, 200);
    gtk_widget_show(imageView);
    
    return (NGHandle)imageView;
}

int ng_linux_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget* imageView = (GtkWidget*)image_view;
    GdkPixbuf* pixbuf = gdk_pixbuf_new_from_file(path, NULL);
    
    if (!pixbuf) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    gtk_image_set_from_pixbuf(GTK_IMAGE(imageView), pixbuf);
    g_object_unref(pixbuf);
    
    return NG_SUCCESS;
}

int ng_linux_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget* imageView = (GtkWidget*)image_view;
    GdkPixbufLoader* loader = gdk_pixbuf_loader_new();
    
    if (!loader) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    if (!gdk_pixbuf_loader_write(loader, data, size, NULL)) {
        g_object_unref(loader);
        return NG_ERROR_CREATION_FAILED;
    }
    
    if (!gdk_pixbuf_loader_close(loader, NULL)) {
        g_object_unref(loader);
        return NG_ERROR_CREATION_FAILED;
    }
    
    GdkPixbuf* pixbuf = gdk_pixbuf_loader_get_pixbuf(loader);
    if (!pixbuf) {
        g_object_unref(loader);
        return NG_ERROR_CREATION_FAILED;
    }
    
    g_object_ref(pixbuf);
    gtk_image_set_from_pixbuf(GTK_IMAGE(imageView), pixbuf);
    g_object_unref(pixbuf);
    g_object_unref(loader);
    
    return NG_SUCCESS;
}

void ng_linux_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    
    GtkWidget* imageView = (GtkWidget*)image_view;
    
    switch (scaling_mode) {
        case IMAGE_SCALING_NONE:
            gtk_image_set_pixel_size(GTK_IMAGE(imageView), -1);
            break;
        case IMAGE_SCALING_ASPECT_FIT:
            gtk_widget_set_halign(imageView, GTK_ALIGN_CENTER);
            gtk_widget_set_valign(imageView, GTK_ALIGN_CENTER);
            break;
        case IMAGE_SCALING_ASPECT_FILL:
            gtk_widget_set_halign(imageView, GTK_ALIGN_FILL);
            gtk_widget_set_valign(imageView, GTK_ALIGN_FILL);
            break;
        case IMAGE_SCALING_FILL:
            gtk_widget_set_halign(imageView, GTK_ALIGN_FILL);
            gtk_widget_set_valign(imageView, GTK_ALIGN_FILL);
            break;
    }
}

void ng_linux_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    gtk_widget_queue_draw((GtkWidget*)image_view);
}

