#include "../elements.h"
#include <gtk/gtk.h>
#include <string.h>

typedef struct {
    int insert;
    int offset;
    char* text;
    int group;
} UndoStep;

typedef struct {
    GArray* undo;
    GArray* redo;
    int enabled;
    int applying;
    int action_depth;
    int action_group;
    int next_group;
    int can_merge;
} UndoStack;

#define UNDO_KEY "aurea-undo"
#define UNDO_LIMIT 500

static void step_clear(gpointer p) {
    g_free(((UndoStep*)p)->text);
}

static void stack_free(gpointer p) {
    UndoStack* stack = (UndoStack*)p;
    g_array_free(stack->undo, TRUE);
    g_array_free(stack->redo, TRUE);
    g_free(stack);
}

static UndoStack* stack_of(GtkTextBuffer* buffer) {
    return buffer ? g_object_get_data(G_OBJECT(buffer), UNDO_KEY) : NULL;
}

static int is_word(const char* text) {
    return g_utf8_strlen(text, -1) == 1 && !g_unichar_isspace(g_utf8_get_char(text));
}

static void record(UndoStack* stack, int insert, int offset, const char* text) {
    if (!stack->enabled || stack->applying || !*text) return;
    g_array_set_size(stack->redo, 0);

    int fresh = stack->action_depth == 0 || stack->action_group == 0;
    UndoStep* last = stack->undo->len
        ? &g_array_index(stack->undo, UndoStep, stack->undo->len - 1) : NULL;
    if (fresh && stack->can_merge && last && insert && is_word(text) &&
        offset == last->offset + (int)g_utf8_strlen(last->text, -1)) {
        char* joined = g_strconcat(last->text, text, NULL);
        g_free(last->text);
        last->text = joined;
        return;
    }

    int group = fresh ? ++stack->next_group : stack->action_group;
    if (stack->action_depth > 0) stack->action_group = group;
    UndoStep step = {insert, offset, g_strdup(text), group};
    g_array_append_val(stack->undo, step);
    if (stack->undo->len > UNDO_LIMIT) g_array_remove_index(stack->undo, 0);
    stack->can_merge = fresh && insert && is_word(text);
}

static void on_insert(GtkTextBuffer* buffer, GtkTextIter* at, gchar* text, gint len, gpointer unused) {
    (void)unused;
    UndoStack* stack = stack_of(buffer);
    if (!stack) return;
    char* copy = g_strndup(text, (gsize)len);
    record(stack, 1, gtk_text_iter_get_offset(at), copy);
    g_free(copy);
}

static void on_delete(GtkTextBuffer* buffer, GtkTextIter* start, GtkTextIter* end, gpointer unused) {
    (void)unused;
    UndoStack* stack = stack_of(buffer);
    if (!stack) return;
    char* text = gtk_text_buffer_get_text(buffer, start, end, TRUE);
    record(stack, 0, gtk_text_iter_get_offset(start), text);
    g_free(text);
}

static void on_begin_action(GtkTextBuffer* buffer, gpointer unused) {
    (void)unused;
    UndoStack* stack = stack_of(buffer);
    if (stack && stack->action_depth++ == 0) stack->action_group = 0;
}

static void on_end_action(GtkTextBuffer* buffer, gpointer unused) {
    (void)unused;
    UndoStack* stack = stack_of(buffer);
    if (stack && stack->action_depth > 0) stack->action_depth--;
}

static void apply(GtkTextBuffer* buffer, const UndoStep* step, int inverse) {
    GtkTextIter start;
    gtk_text_buffer_get_iter_at_offset(buffer, &start, step->offset);
    if (step->insert != inverse) {
        gtk_text_buffer_insert(buffer, &start, step->text, -1);
    } else {
        GtkTextIter end;
        gtk_text_buffer_get_iter_at_offset(buffer, &end, step->offset + (int)g_utf8_strlen(step->text, -1));
        gtk_text_buffer_delete(buffer, &start, &end);
    }
    gtk_text_buffer_get_iter_at_offset(buffer, &start, step->offset +
        (step->insert != inverse ? (int)g_utf8_strlen(step->text, -1) : 0));
    gtk_text_buffer_place_cursor(buffer, &start);
}

static int move_group(GtkTextBuffer* buffer, UndoStack* stack, int redo) {
    GArray* from = redo ? stack->redo : stack->undo;
    GArray* to = redo ? stack->undo : stack->redo;
    if (!from->len) return 0;
    int group = g_array_index(from, UndoStep, from->len - 1).group;
    stack->applying = 1;
    while (from->len && g_array_index(from, UndoStep, from->len - 1).group == group) {
        UndoStep step = g_array_index(from, UndoStep, from->len - 1);
        g_array_set_clear_func(from, NULL);
        g_array_remove_index(from, from->len - 1);
        g_array_set_clear_func(from, step_clear);
        apply(buffer, &step, !redo);
        g_array_append_val(to, step);
    }
    stack->applying = 0;
    stack->can_merge = 0;
    return 1;
}

void ng_linux_undo_attach(void* text_buffer) {
    GtkTextBuffer* buffer = GTK_TEXT_BUFFER(text_buffer);
    UndoStack* stack = g_new0(UndoStack, 1);
    stack->undo = g_array_new(FALSE, FALSE, sizeof(UndoStep));
    stack->redo = g_array_new(FALSE, FALSE, sizeof(UndoStep));
    g_array_set_clear_func(stack->undo, step_clear);
    g_array_set_clear_func(stack->redo, step_clear);
    stack->enabled = 1;
    g_object_set_data_full(G_OBJECT(buffer), UNDO_KEY, stack, stack_free);
    g_signal_connect(buffer, "insert-text", G_CALLBACK(on_insert), NULL);
    g_signal_connect(buffer, "delete-range", G_CALLBACK(on_delete), NULL);
    g_signal_connect(buffer, "begin-user-action", G_CALLBACK(on_begin_action), NULL);
    g_signal_connect(buffer, "end-user-action", G_CALLBACK(on_end_action), NULL);
}

int ng_linux_undo(void* text_buffer, int redo) {
    GtkTextBuffer* buffer = GTK_TEXT_BUFFER(text_buffer);
    UndoStack* stack = stack_of(buffer);
    return stack && stack->enabled && move_group(buffer, stack, redo);
}

int ng_linux_can_undo(void* text_buffer, int redo) {
    UndoStack* stack = stack_of(GTK_TEXT_BUFFER(text_buffer));
    return stack && stack->enabled && (redo ? stack->redo->len : stack->undo->len) > 0;
}

void ng_linux_undo_clear(void* text_buffer) {
    UndoStack* stack = stack_of(GTK_TEXT_BUFFER(text_buffer));
    if (!stack) return;
    g_array_set_size(stack->undo, 0);
    g_array_set_size(stack->redo, 0);
    stack->can_merge = 0;
}

int ng_linux_undo_enable(void* text_buffer, int on) {
    UndoStack* stack = stack_of(GTK_TEXT_BUFFER(text_buffer));
    if (!stack) return 0;
    stack->enabled = on;
    if (!on) ng_linux_undo_clear(text_buffer);
    return 1;
}

static gboolean on_undo_keys(GtkWidget* view, GdkEventKey* event, gpointer unused) {
    (void)unused;
    if (!(event->state & GDK_CONTROL_MASK)) return FALSE;
    GtkTextBuffer* buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));
    guint key = gdk_keyval_to_lower(event->keyval);
    if (key == GDK_KEY_z) {
        ng_linux_undo(buffer, (event->state & GDK_SHIFT_MASK) != 0);
        return TRUE;
    }
    if (key == GDK_KEY_y) {
        ng_linux_undo(buffer, 1);
        return TRUE;
    }
    return FALSE;
}

void ng_linux_undo_keys(void* text_view) {
    g_signal_connect(text_view, "key-press-event", G_CALLBACK(on_undo_keys), NULL);
}
