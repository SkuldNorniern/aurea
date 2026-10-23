#include "../elements.h"
#include "common/errors.h"
#include "common/platform_api.h"
#include <gtk/gtk.h>

/* A wrapping label's minimum width is its longest word; a window takes its minimum height at its
   minimum width, so every word gets a line and the window grows tall. A line this long or shorter
   stays whole. */
#define LABEL_MIN_CHARS 40

static void keep_short_lines(GtkLabel* label, const char* text) {
    glong longest = 0, run = 0;
    for (const char* at = text; *at; at = g_utf8_next_char(at)) {
        run = *at == '\n' ? 0 : run + 1;
        if (run > longest) longest = run;
    }
    gtk_label_set_width_chars(label, (gint)MIN(longest, LABEL_MIN_CHARS));
}

NGHandle ng_linux_create_label(const char* text) {
    if (!text) return NULL;
    
    GtkWidget* label = gtk_label_new(text);
    gtk_label_set_line_wrap(GTK_LABEL(label), TRUE);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    keep_short_lines(GTK_LABEL(label), text);
    gtk_widget_show(label);
    
    return (NGHandle)label;
}

int ng_linux_label_set_text(NGHandle label, const char* text) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    gtk_label_set_text(GTK_LABEL(label), text);
    keep_short_lines(GTK_LABEL(label), text);
    return NG_SUCCESS;
}

int ng_linux_label_set_style(NGHandle label, int style) {
    if (!label || !GTK_IS_LABEL(label)) return NG_ERROR_INVALID_HANDLE;
    if (style < NG_TEXT_STYLE_BODY || style > NG_TEXT_STYLE_HEADING) return NG_ERROR_INVALID_PARAMETER;
    PangoAttrList* attrs = pango_attr_list_new();
    if (style == NG_TEXT_STYLE_STRONG || style == NG_TEXT_STYLE_HEADING) {
        pango_attr_list_insert(attrs, pango_attr_weight_new(PANGO_WEIGHT_SEMIBOLD));
    }
    if (style == NG_TEXT_STYLE_SMALL) pango_attr_list_insert(attrs, pango_attr_scale_new(0.9));
    if (style == NG_TEXT_STYLE_HEADING) pango_attr_list_insert(attrs, pango_attr_scale_new(1.4));
    gtk_label_set_attributes(GTK_LABEL(label), attrs);
    pango_attr_list_unref(attrs);
    return NG_SUCCESS;
}

void ng_linux_label_invalidate(NGHandle label) {
    if (!label) return;
    gtk_widget_queue_draw((GtkWidget*)label);
}

