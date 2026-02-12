#include "../elements.h"
#include "common/errors.h"
#include <gtk/gtk.h>

extern void ng_invoke_textview_callback(unsigned int id, const char* content);

static void text_buffer_changed(GtkTextBuffer* buffer, gpointer user_data) {
    unsigned int id = GPOINTER_TO_UINT(user_data);
    GtkTextIter start, end;
    gtk_text_buffer_get_start_iter(buffer, &start);
    gtk_text_buffer_get_end_iter(buffer, &end);
    gchar* text = gtk_text_buffer_get_text(buffer, &start, &end, FALSE);
    if (text) {
        ng_invoke_textview_callback(id, text);
        g_free(text);
    }
}

NGHandle ng_linux_create_text_view(int is_editable, unsigned int id) {
    GtkWidget* scrolled_window = gtk_scrolled_window_new(NULL, NULL);
    GtkWidget* text_view = gtk_text_view_new();
    
    gtk_text_view_set_editable(GTK_TEXT_VIEW(text_view), is_editable ? TRUE : FALSE);
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(text_view), GTK_WRAP_WORD);
    
    if (id != 0 && is_editable) {
        GtkTextBuffer* buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(text_view));
        g_signal_connect(buffer, "changed", G_CALLBACK(text_buffer_changed), GUINT_TO_POINTER(id));
    }
    
    gtk_container_add(GTK_CONTAINER(scrolled_window), text_view);
    gtk_widget_show_all(scrolled_window);
    
    return (NGHandle)scrolled_window;
}

void ng_linux_text_view_invalidate(NGHandle text_view) {
    if (!text_view) return;
    gtk_widget_queue_draw((GtkWidget*)text_view);
}

