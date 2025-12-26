#include "../elements.h"
#include "../../common/errors.h"
#include <gtk/gtk.h>
#include <stdlib.h>

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

