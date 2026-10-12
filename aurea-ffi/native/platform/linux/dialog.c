/* Open, save and folder dialogs: GtkFileChooserNative, which is the portal's
   dialog under Flatpak and the desktop's own elsewhere. */

#include "dialog.h"
#include <gtk/gtk.h>
#include <stdlib.h>
#include <string.h>

/* Adds a filter for each "Images|png;jpg" line. */
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
            gchar* pattern = strcmp(*ext, "*") == 0 ? g_strdup("*") : g_strconcat("*.", *ext, NULL);
            gtk_file_filter_add_pattern(filter, pattern);
            g_free(pattern);
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
        GString* joined = g_string_new(NULL);
        for (GSList* l = names; l; l = l->next) {
            if (joined->len) g_string_append_c(joined, '\n');
            g_string_append(joined, (const char*)l->data);
        }
        g_slist_free_full(names, g_free);
        if (joined->len) out = strdup(joined->str);
        g_string_free(joined, TRUE);
    }
    g_object_unref(native);
    return out;
}
