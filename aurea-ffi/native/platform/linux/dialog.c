
#include "dialog.h"
#include <gtk/gtk.h>
#include <stdlib.h>
#include <string.h>

char* ng_linux_filter_pattern(const char* ext) {
    if (strcmp(ext, "*") == 0) return g_strdup("*");
    GString* out = g_string_new("*.");
    for (const char* c = ext; *c; c++) {
        if (g_ascii_isalpha(*c)) {
            g_string_append_printf(out, "[%c%c]", g_ascii_tolower(*c), g_ascii_toupper(*c));
        } else {
            g_string_append_c(out, *c);
        }
    }
    return g_string_free(out, FALSE);
}

char* ng_linux_with_extension(const char* path, const char* ext) {
    const char* slash = strrchr(path, '/');
    const char* base = slash ? slash + 1 : path;
    if (!ext || !*ext || strcmp(ext, "*") == 0 || !*base || strchr(base, '.')) return g_strdup(path);
    return g_strconcat(path, ".", ext, NULL);
}

static void add_filters(GtkFileChooser* chooser, const char* filters) {
    if (!filters || !*filters) return;
    gchar** lines = g_strsplit(filters, "\n", -1);
    for (gchar** line = lines; *line; line++) {
        gchar* bar = strchr(*line, '|');
        if (!bar) continue;
        *bar = '\0';
        GtkFileFilter* filter = gtk_file_filter_new();
        gtk_file_filter_set_name(filter, *line);
        gchar** exts = g_strsplit(bar + 1, ";", -1);
        for (gchar** ext = exts; *ext; ext++) {
            if (!**ext) continue;
            gchar* pattern = ng_linux_filter_pattern(*ext);
            gtk_file_filter_add_pattern(filter, pattern);
            g_free(pattern);
            if (!g_object_get_data(G_OBJECT(filter), "ng-ext")) {
                g_object_set_data_full(G_OBJECT(filter), "ng-ext", g_strdup(*ext), g_free);
            }
        }
        g_strfreev(exts);
        gtk_file_chooser_add_filter(chooser, filter);
    }
    g_strfreev(lines);
}

char* ng_linux_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                           const char* default_name, const char* directory, int multiple) {
    GtkFileChooserAction action = kind == NG_DIALOG_SAVE ? GTK_FILE_CHOOSER_ACTION_SAVE
        : kind == NG_DIALOG_FOLDER ? GTK_FILE_CHOOSER_ACTION_SELECT_FOLDER
        : GTK_FILE_CHOOSER_ACTION_OPEN;
    const char* accept = kind == NG_DIALOG_SAVE ? "_Save" : "_Open";
    GtkWindow* parent = window ? GTK_WINDOW(window) : NULL;
    GtkFileChooserNative* native = gtk_file_chooser_native_new(
        title && *title ? title : NULL, parent, action, accept, "_Cancel");
    GtkFileChooser* chooser = GTK_FILE_CHOOSER(native);
    if (kind == NG_DIALOG_OPEN) gtk_file_chooser_set_select_multiple(chooser, multiple ? TRUE : FALSE);
    if (kind == NG_DIALOG_SAVE) {
        gtk_file_chooser_set_do_overwrite_confirmation(chooser, TRUE);
        if (default_name && *default_name) gtk_file_chooser_set_current_name(chooser, default_name);
    }
    if (kind != NG_DIALOG_FOLDER) add_filters(chooser, filters);
    if (directory && *directory) gtk_file_chooser_set_current_folder(chooser, directory);

    char* out = NULL;
    if (gtk_native_dialog_run(GTK_NATIVE_DIALOG(native)) == GTK_RESPONSE_ACCEPT) {
        GSList* names = gtk_file_chooser_get_filenames(chooser);
        if (kind == NG_DIALOG_SAVE && names) {
            GtkFileFilter* chosen = gtk_file_chooser_get_filter(chooser);
            const char* ext = chosen ? g_object_get_data(G_OBJECT(chosen), "ng-ext") : NULL;
            char* named = ng_linux_with_extension((const char*)names->data, ext);
            if (strcmp(named, (const char*)names->data) != 0 && g_file_test(named, G_FILE_TEST_EXISTS)) {
                gchar* base = g_path_get_basename(named);
                gchar* ask = g_strdup_printf("\"%s\" already exists. Replace it?", base);
                int replace = ng_linux_message_box(window, NG_MESSAGE_WARNING, title, ask,
                                                   NG_MESSAGE_BUTTONS_YES_NO) == NG_CHOICE_YES;
                g_free(ask);
                g_free(base);
                if (!replace) {
                    g_free(named);
                    named = NULL;
                }
            }
            g_free(names->data);
            names->data = named;
        }
        GString* joined = g_string_new(NULL);
        for (GSList* l = names; l; l = l->next) {
            const char* name = (const char*)l->data;
            if (name && *name) g_string_append_len(joined, name, (gssize)strlen(name) + 1);
        }
        g_slist_free_full(names, g_free);
        if (joined->len) {
            out = (char*)malloc(joined->len + 1);
            if (out) {
                memcpy(out, joined->str, joined->len);
                out[joined->len] = '\0';
            }
        }
        g_string_free(joined, TRUE);
    }
    g_object_unref(native);
    return out;
}

#define RESPONSE_SAVE 100
#define RESPONSE_DISCARD 101

static void add_buttons(GtkDialog* dialog, int buttons) {
    switch (buttons) {
    case NG_MESSAGE_BUTTONS_OK_CANCEL:
        gtk_dialog_add_button(dialog, "_Cancel", GTK_RESPONSE_CANCEL);
        gtk_dialog_add_button(dialog, "_OK", GTK_RESPONSE_OK);
        gtk_dialog_set_default_response(dialog, GTK_RESPONSE_OK);
        break;
    case NG_MESSAGE_BUTTONS_YES_NO:
        gtk_dialog_add_button(dialog, "_No", GTK_RESPONSE_NO);
        gtk_dialog_add_button(dialog, "_Yes", GTK_RESPONSE_YES);
        gtk_dialog_set_default_response(dialog, GTK_RESPONSE_YES);
        break;
    case NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL:
        gtk_dialog_add_button(dialog, "_Don't Save", RESPONSE_DISCARD);
        gtk_dialog_add_button(dialog, "_Cancel", GTK_RESPONSE_CANCEL);
        gtk_dialog_add_button(dialog, "_Save", RESPONSE_SAVE);
        gtk_dialog_set_default_response(dialog, RESPONSE_SAVE);
        break;
    default:
        gtk_dialog_add_button(dialog, "_OK", GTK_RESPONSE_OK);
        gtk_dialog_set_default_response(dialog, GTK_RESPONSE_OK);
        break;
    }
}

static int choice_of(gint response, int buttons) {
    switch (response) {
    case GTK_RESPONSE_OK: return NG_CHOICE_OK;
    case GTK_RESPONSE_CANCEL: return NG_CHOICE_CANCEL;
    case GTK_RESPONSE_YES: return NG_CHOICE_YES;
    case GTK_RESPONSE_NO: return NG_CHOICE_NO;
    case RESPONSE_SAVE: return NG_CHOICE_SAVE;
    case RESPONSE_DISCARD: return NG_CHOICE_DISCARD;
    default: break;
    }
    if (buttons == NG_MESSAGE_BUTTONS_YES_NO) return NG_CHOICE_NO;
    if (buttons == NG_MESSAGE_BUTTONS_OK) return NG_CHOICE_OK;
    return NG_CHOICE_CANCEL;
}

int ng_linux_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons) {
    GtkWindow* parent = window ? GTK_WINDOW(window) : NULL;
    GtkMessageType type = kind == NG_MESSAGE_ERROR ? GTK_MESSAGE_ERROR
        : kind == NG_MESSAGE_WARNING ? GTK_MESSAGE_WARNING
        : GTK_MESSAGE_INFO;
    GtkWidget* dialog = gtk_message_dialog_new(parent, GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
                                               type, GTK_BUTTONS_NONE, "%s", text ? text : "");
    if (title && *title) gtk_window_set_title(GTK_WINDOW(dialog), title);
    add_buttons(GTK_DIALOG(dialog), buttons);

    gint response = gtk_dialog_run(GTK_DIALOG(dialog));
    gtk_widget_destroy(dialog);
    return choice_of(response, buttons);
}

static unsigned int channel_of(double v) {
    double scaled = v * 255.0 + 0.5;
    if (scaled < 0) return 0;
    if (scaled > 255) return 255;
    return (unsigned int)scaled;
}

int ng_linux_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out) {
    GtkWindow* parent = window ? GTK_WINDOW(window) : NULL;
    GtkWidget* dialog = gtk_color_chooser_dialog_new(title && *title ? title : "Pick a Color", parent);
    gtk_window_set_modal(GTK_WINDOW(dialog), TRUE);
    gtk_window_set_destroy_with_parent(GTK_WINDOW(dialog), TRUE);
    GtkColorChooser* chooser = GTK_COLOR_CHOOSER(dialog);
    gtk_color_chooser_set_use_alpha(chooser, alpha ? TRUE : FALSE);
    GdkRGBA start = {
        .red = ((argb_in >> 16) & 0xFF) / 255.0,
        .green = ((argb_in >> 8) & 0xFF) / 255.0,
        .blue = (argb_in & 0xFF) / 255.0,
        .alpha = alpha ? ((argb_in >> 24) & 0xFF) / 255.0 : 1.0,
    };
    gtk_color_chooser_set_rgba(chooser, &start);

    int result = 0;
    if (gtk_dialog_run(GTK_DIALOG(dialog)) == GTK_RESPONSE_OK) {
        GdkRGBA picked;
        gtk_color_chooser_get_rgba(chooser, &picked);
        unsigned int a = alpha ? channel_of(picked.alpha) : (argb_in >> 24) & 0xFF;
        *argb_out = (a << 24) | (channel_of(picked.red) << 16) | (channel_of(picked.green) << 8) | channel_of(picked.blue);
        result = 1;
    }
    gtk_widget_destroy(dialog);
    return result;
}
