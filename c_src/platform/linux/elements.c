#include "elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>
#include <string.h>
#include <stdlib.h>

NGHandle ng_linux_create_button(const char* title) {
    if (!title) return NULL;
    
    GtkWidget* button = gtk_button_new_with_label(title);
    gtk_widget_show(button);
    
    return (NGHandle)button;
}

NGHandle ng_linux_create_label(const char* text) {
    if (!text) return NULL;
    
    GtkWidget* label = gtk_label_new(text);
    gtk_widget_show(label);
    
    return (NGHandle)label;
}

NGHandle ng_linux_create_box(int is_vertical) {
    GtkOrientation orientation = is_vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL;
    GtkWidget* box = gtk_box_new(orientation, 8);
    gtk_widget_show(box);
    
    return (NGHandle)box;
}

int ng_linux_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    gtk_container_add(GTK_CONTAINER(box), (GtkWidget*)element);
    gtk_widget_show((GtkWidget*)element);
    
    return NG_SUCCESS;
}

NGHandle ng_linux_create_text_editor(void) {
    GtkWidget* scrolled_window = gtk_scrolled_window_new(NULL, NULL);
    GtkWidget* text_view = gtk_text_view_new();
    
    gtk_text_view_set_editable(GTK_TEXT_VIEW(text_view), TRUE);
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(text_view), GTK_WRAP_WORD);
    
    gtk_container_add(GTK_CONTAINER(scrolled_window), text_view);
    gtk_widget_show_all(scrolled_window);
    
    return (NGHandle)scrolled_window;
}

NGHandle ng_linux_create_text_view(int is_editable) {
    GtkWidget* scrolled_window = gtk_scrolled_window_new(NULL, NULL);
    GtkWidget* text_view = gtk_text_view_new();
    
    gtk_text_view_set_editable(GTK_TEXT_VIEW(text_view), is_editable ? TRUE : FALSE);
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(text_view), GTK_WRAP_WORD);
    
    gtk_container_add(GTK_CONTAINER(scrolled_window), text_view);
    gtk_widget_show_all(scrolled_window);
    
    return (NGHandle)scrolled_window;
}

int ng_linux_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    
    GtkWidget* scrolled = (GtkWidget*)text_handle;
    GtkWidget* child = gtk_bin_get_child(GTK_BIN(scrolled));
    
    if (GTK_IS_TEXT_VIEW(child)) {
        GtkTextBuffer* buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(child));
        gtk_text_buffer_set_text(buffer, content, -1);
        return NG_SUCCESS;
    }
    
    return NG_ERROR_INVALID_HANDLE;
}

char* ng_linux_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    GtkWidget* scrolled = (GtkWidget*)text_handle;
    GtkWidget* child = gtk_bin_get_child(GTK_BIN(scrolled));
    
    if (GTK_IS_TEXT_VIEW(child)) {
        GtkTextBuffer* buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(child));
        GtkTextIter start, end;
        gtk_text_buffer_get_start_iter(buffer, &start);
        gtk_text_buffer_get_end_iter(buffer, &end);
        
        char* text = gtk_text_buffer_get_text(buffer, &start, &end, FALSE);
        return text;
    }
    
    return NULL;
}

void ng_linux_free_text_content(char* content) {
    if (content) {
        g_free(content);
    }
}

NGHandle ng_linux_create_canvas(int width, int height) {
    // Create a GtkDrawingArea for custom rendering
    // This will be extended to support OpenGL/Vulkan
    GtkWidget* drawing_area = gtk_drawing_area_new();
    gtk_widget_set_size_request(drawing_area, width, height);
    gtk_widget_show(drawing_area);
    
    return (NGHandle)drawing_area;
}

void ng_linux_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    gtk_widget_queue_draw((GtkWidget*)canvas);
}

