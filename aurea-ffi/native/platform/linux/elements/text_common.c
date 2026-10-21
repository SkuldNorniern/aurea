#include "../elements.h"
#include "common/errors.h"
#include "common/platform_api.h"
#include <gtk/gtk.h>
#include <stdlib.h>

NGHandle ng_linux_create_text_field(void) {
    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "");
    gtk_widget_set_hexpand(entry, TRUE);
    gtk_widget_show(entry);
    return (NGHandle)entry;
}

NGHandle ng_linux_create_text_area(int editable) {
    GtkWidget* scrolled_window = gtk_scrolled_window_new(NULL, NULL);
    GtkWidget* text_view = gtk_text_view_new();
    gtk_text_view_set_editable(GTK_TEXT_VIEW(text_view), editable ? TRUE : FALSE);
    gtk_container_add(GTK_CONTAINER(scrolled_window), text_view);
    ng_linux_undo_attach(gtk_text_view_get_buffer(GTK_TEXT_VIEW(text_view)));
    ng_linux_undo_keys(text_view);
    gtk_widget_show_all(scrolled_window);
    return (NGHandle)scrolled_window;
}

static GtkTextView* text_view_of(NGHandle handle) {
    GtkWidget* widget = (GtkWidget*)handle;
    if (!widget || !GTK_IS_BIN(widget)) return NULL;
    GtkWidget* child = gtk_bin_get_child(GTK_BIN(widget));
    return GTK_IS_TEXT_VIEW(child) ? GTK_TEXT_VIEW(child) : NULL;
}

static GtkEntry* entry_of(NGHandle handle) {
    return handle && GTK_IS_ENTRY(handle) ? GTK_ENTRY(handle) : NULL;
}

int ng_linux_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;

    GtkEntry* entry = entry_of(text_handle);
    if (entry) {
        ng_linux_set_quiet(entry, 1);
        gtk_entry_set_text(entry, content);
        ng_linux_set_quiet(entry, 0);
        return NG_SUCCESS;
    }

    GtkTextView* view = text_view_of(text_handle);
    if (view) {
        GtkTextBuffer* buffer = gtk_text_view_get_buffer(view);
        ng_linux_set_quiet(buffer, 1);
        gtk_text_buffer_set_text(buffer, content, -1);
        ng_linux_set_quiet(buffer, 0);
        ng_linux_undo_clear(buffer);
        return NG_SUCCESS;
    }

    return NG_ERROR_INVALID_HANDLE;
}

char* ng_linux_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;

    GtkEntry* entry = entry_of(text_handle);
    if (entry) {
        const char* text = gtk_entry_get_text(entry);
        return text ? g_strdup(text) : NULL;
    }

    GtkTextView* view = text_view_of(text_handle);
    if (view) {
        GtkTextBuffer* buffer = gtk_text_view_get_buffer(view);
        GtkTextIter start, end;
        gtk_text_buffer_get_bounds(buffer, &start, &end);
        return gtk_text_buffer_get_text(buffer, &start, &end, FALSE);
    }

    return NULL;
}

void ng_linux_free_text_content(char* content) {
    if (content) {
        g_free(content);
    }
}

int ng_linux_text_unit(void) {
    return NG_TEXT_UNIT_CHAR;
}

int ng_linux_text_get_selection(NGHandle text, int* start, int* end) {
    if (!start || !end) return NG_ERROR_INVALID_PARAMETER;
    GtkEntry* entry = entry_of(text);
    if (entry) {
        gtk_editable_get_selection_bounds(GTK_EDITABLE(entry), start, end);
        return NG_SUCCESS;
    }
    GtkTextView* view = text_view_of(text);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    GtkTextIter from, to;
    gtk_text_buffer_get_selection_bounds(gtk_text_view_get_buffer(view), &from, &to);
    *start = gtk_text_iter_get_offset(&from);
    *end = gtk_text_iter_get_offset(&to);
    return NG_SUCCESS;
}

static int text_length(NGHandle text) {
    GtkEntry* entry = entry_of(text);
    if (entry) return (int)gtk_entry_get_text_length(entry);
    GtkTextView* view = text_view_of(text);
    return view ? gtk_text_buffer_get_char_count(gtk_text_view_get_buffer(view)) : -1;
}

int ng_linux_text_set_selection(NGHandle text, int start, int end) {
    int length = text_length(text);
    if (length < 0) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || end > length) return NG_ERROR_INVALID_PARAMETER;
    GtkEntry* entry = entry_of(text);
    if (entry) {
        gtk_editable_select_region(GTK_EDITABLE(entry), start, end);
        return NG_SUCCESS;
    }
    GtkTextBuffer* buffer = gtk_text_view_get_buffer(text_view_of(text));
    GtkTextIter from, to;
    gtk_text_buffer_get_iter_at_offset(buffer, &from, start);
    gtk_text_buffer_get_iter_at_offset(buffer, &to, end);
    gtk_text_buffer_select_range(buffer, &to, &from);
    return NG_SUCCESS;
}

int ng_linux_text_replace(NGHandle text, int start, int end, const char* replacement) {
    if (!replacement) return NG_ERROR_INVALID_PARAMETER;
    int length = text_length(text);
    if (length < 0) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || end > length) return NG_ERROR_INVALID_PARAMETER;

    GtkEntry* entry = entry_of(text);
    if (entry) {
        GtkEditable* editable = GTK_EDITABLE(entry);
        ng_linux_set_quiet(entry, 1);
        gtk_editable_delete_text(editable, start, end);
        int at = start;
        gtk_editable_insert_text(editable, replacement, -1, &at);
        gtk_editable_set_position(editable, at);
        ng_linux_set_quiet(entry, 0);
        return NG_SUCCESS;
    }

    GtkTextBuffer* buffer = gtk_text_view_get_buffer(text_view_of(text));
    GtkTextIter from, to;
    gtk_text_buffer_get_iter_at_offset(buffer, &from, start);
    gtk_text_buffer_get_iter_at_offset(buffer, &to, end);
    ng_linux_set_quiet(buffer, 1);
    gtk_text_buffer_begin_user_action(buffer);
    gtk_text_buffer_delete(buffer, &from, &to);
    gtk_text_buffer_insert(buffer, &from, replacement, -1);
    gtk_text_buffer_end_user_action(buffer);
    gtk_text_buffer_place_cursor(buffer, &from);
    ng_linux_set_quiet(buffer, 0);
    return NG_SUCCESS;
}

int ng_linux_text_undo(NGHandle text, int redo) {
    GtkTextView* view = text_view_of(text);
    if (!view) return entry_of(text) ? NG_ERROR_UNSUPPORTED : NG_ERROR_INVALID_HANDLE;
    GtkTextBuffer* buffer = gtk_text_view_get_buffer(view);
    ng_linux_set_quiet(buffer, 1);
    int undone = ng_linux_undo(buffer, redo);
    ng_linux_set_quiet(buffer, 0);
    return undone ? NG_SUCCESS : NG_ERROR_INVALID_PARAMETER;
}

int ng_linux_text_can_undo(NGHandle text, int redo) {
    GtkTextView* view = text_view_of(text);
    return view ? ng_linux_can_undo(gtk_text_view_get_buffer(view), redo) : 0;
}

int ng_linux_text_set_feature(NGHandle text, int feature, int on) {
    GtkEntry* entry = entry_of(text);
    GtkTextView* view = text_view_of(text);
    if (!entry && !view) return NG_ERROR_INVALID_HANDLE;
    switch (feature) {
        case NG_TEXT_FEATURE_READ_ONLY:
            if (entry) {
                gtk_editable_set_editable(GTK_EDITABLE(entry), !on);
            } else {
                gtk_text_view_set_editable(view, !on);
                gtk_text_view_set_cursor_visible(view, !on);
            }
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_WRAP:
            if (!view) return NG_ERROR_UNSUPPORTED;
            gtk_text_view_set_wrap_mode(view, on ? GTK_WRAP_WORD_CHAR : GTK_WRAP_NONE);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_MONOSPACE:
            if (view) {
                gtk_text_view_set_monospace(view, on);
            } else {
                GtkStyleContext* style = gtk_widget_get_style_context(GTK_WIDGET(entry));
                if (on) gtk_style_context_add_class(style, "monospace");
                else gtk_style_context_remove_class(style, "monospace");
            }
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_UNDO:
            if (!view) return NG_ERROR_UNSUPPORTED;
            ng_linux_undo_enable(gtk_text_view_get_buffer(view), on);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_SECRET:
            if (!entry) return NG_ERROR_UNSUPPORTED;
            gtk_entry_set_visibility(entry, !on);
            gtk_entry_set_input_purpose(entry, on ? GTK_INPUT_PURPOSE_PASSWORD : GTK_INPUT_PURPOSE_FREE_FORM);
            return NG_SUCCESS;
        default:
            return NG_ERROR_INVALID_PARAMETER;
    }
}
