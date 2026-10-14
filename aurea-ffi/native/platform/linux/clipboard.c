#include "common/clipboard.h"
#include <gtk/gtk.h>

typedef struct {
    size_t count;
    NGClipboardEntry* entries;
} ClipboardPayload;

static void clear_payload(GtkClipboard* clipboard, gpointer user_data) {
    (void)clipboard;
    ClipboardPayload* payload = (ClipboardPayload*)user_data;
    for (size_t i = 0; i < payload->count; ++i) {
        free((void*)payload->entries[i].format);
        free((void*)payload->entries[i].data);
    }
    free(payload->entries); free(payload);
}
static void get_payload(GtkClipboard* clipboard, GtkSelectionData* selection, guint info, gpointer user_data) {
    (void)clipboard;
    ClipboardPayload* payload = (ClipboardPayload*)user_data;
    if (info >= payload->count) return;
    const NGClipboardEntry* item = &payload->entries[info];
    gtk_selection_data_set(selection, gtk_selection_data_get_target(selection), 8, item->data, (gint)item->len);
}

int ng_linux_clipboard_write(const NGClipboardEntry* entries, size_t count) {
    if (!gdk_display_get_default()) return NG_ERROR_PLATFORM_SPECIFIC;
    if (!count) { gtk_clipboard_clear(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD)); return NG_SUCCESS; }
    if (count > G_MAXINT || count > SIZE_MAX / sizeof(NGClipboardEntry) || count > SIZE_MAX / sizeof(GtkTargetEntry))
        return NG_ERROR_INVALID_PARAMETER;
    ClipboardPayload* payload = (ClipboardPayload*)calloc(1, sizeof(*payload));
    if (!payload) return NG_ERROR_PLATFORM_SPECIFIC;
    payload->entries = (NGClipboardEntry*)calloc(count, sizeof(NGClipboardEntry));
    GtkTargetEntry* targets = (GtkTargetEntry*)calloc(count, sizeof(GtkTargetEntry));
    if (!payload->entries || !targets) { free(targets); clear_payload(NULL, payload); return NG_ERROR_PLATFORM_SPECIFIC; }
    payload->count = count;
    int result = NG_SUCCESS;
    for (size_t i = 0; i < count; ++i) {
        if (!entries[i].format || entries[i].len > G_MAXINT || (entries[i].len && !entries[i].data)) {
            result = NG_ERROR_INVALID_PARAMETER; break;
        }
        char* name = strdup(entries[i].format);
        unsigned char* bytes = (unsigned char*)malloc(entries[i].len ? entries[i].len : 1);
        payload->entries[i].format = name;
        payload->entries[i].data = bytes;
        payload->entries[i].len = entries[i].len;
        if (!name || !bytes) { result = NG_ERROR_PLATFORM_SPECIFIC; break; }
        if (entries[i].len) memcpy(bytes, entries[i].data, entries[i].len);
        targets[i].target = name; targets[i].info = (guint)i;
    }
    GtkClipboard* clipboard = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    if (!result && !gtk_clipboard_set_with_data(clipboard, targets, (guint)count, get_payload, clear_payload, payload))
        result = NG_ERROR_PLATFORM_SPECIFIC;
    if (result) clear_payload(NULL, payload);
    else gtk_clipboard_set_can_store(clipboard, targets, (gint)count);
    free(targets);
    return result;
}

int ng_linux_clipboard_read(const char* format, size_t max_bytes, unsigned char** data, size_t* len) {
    if (!gdk_display_get_default()) return NG_ERROR_PLATFORM_SPECIFIC;
    GtkClipboard* clipboard = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
    GtkSelectionData* selection = gtk_clipboard_wait_for_contents(clipboard, gdk_atom_intern(format, FALSE));
    if (!selection) return NG_SUCCESS;
    gint size = gtk_selection_data_get_length(selection);
    int result = NG_SUCCESS;
    if (size >= 0) {
        if ((size_t)size > max_bytes) result = NG_ERROR_LIMIT_EXCEEDED;
        else {
            unsigned char* copy = (unsigned char*)malloc(size ? (size_t)size : 1);
            if (!copy) result = NG_ERROR_PLATFORM_SPECIFIC;
            else {
                if (size) memcpy(copy, gtk_selection_data_get_data(selection), (size_t)size);
                *data = copy; *len = (size_t)size;
            }
        }
    }
    gtk_selection_data_free(selection);
    return result;
}

int ng_linux_clipboard_formats(size_t max_bytes, unsigned char** data, size_t* len) {
    if (!gdk_display_get_default()) return NG_ERROR_PLATFORM_SPECIFIC;
    GdkAtom* atoms = NULL; gint count = 0;
    if (!gtk_clipboard_wait_for_targets(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), &atoms, &count))
        return NG_SUCCESS;
    int result = NG_SUCCESS;
    for (gint i = 0; !result && i < count; ++i) {
        char* name = gdk_atom_name(atoms[i]);
        if (name) { result = ng_clipboard_append(data, len, name, max_bytes); g_free(name); }
    }
    g_free(atoms);
    return result;
}
