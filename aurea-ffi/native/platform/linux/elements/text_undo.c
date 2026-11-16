#include "../elements.h"
#include "../control.h"
#include <gtk/gtk.h>
#include <string.h>

#define UNDO_LIMIT 500

typedef struct {
    int insert;
    int offset;
    char* text;
    int group;
} UndoStep;

/* The buffer's control, so its quiet flag and change id live with its undo history. */
typedef struct {
    NgControl control;
    GQueue undo;
    GQueue redo;
    int enabled;
    int applying;
    int action_depth;
    int action_group;
    int next_group;
    int can_merge;
} UndoStack;

static void step_free(UndoStep* step) {
    if (!step) return;
    g_free(step->text);
    g_free(step);
}

static void clear_steps(GQueue* queue) {
    UndoStep* step;
    while ((step = g_queue_pop_head(queue))) step_free(step);
}

static void stack_free(gpointer data) {
    UndoStack* stack = data;
    clear_steps(&stack->undo);
    clear_steps(&stack->redo);
    g_free(stack);
}

static const char undo_kind = 0;

static UndoStack* stack_of(void* text_buffer) {
    return GTK_IS_TEXT_BUFFER(text_buffer) ? ng_control_typed(text_buffer, &undo_kind) : NULL;
}

static int is_word(const char* text) {
    return g_utf8_strlen(text, -1) == 1 && !g_unichar_isspace(g_utf8_get_char(text));
}

static void record(UndoStack* stack, int insert, int offset, const char* text) {
    if (!stack->enabled || stack->applying || !*text) return;
    clear_steps(&stack->redo);

    int fresh = stack->action_depth == 0 || stack->action_group == 0;
    UndoStep* last = g_queue_peek_tail(&stack->undo);
    if (fresh && stack->can_merge && last && insert && is_word(text) &&
        offset == last->offset + (int)g_utf8_strlen(last->text, -1)) {
        char* joined = g_strconcat(last->text, text, NULL);
        g_free(last->text);
        last->text = joined;
        return;
    }

    int group = fresh ? ++stack->next_group : stack->action_group;
    if (stack->action_depth > 0) stack->action_group = group;
    UndoStep* step = g_new0(UndoStep, 1);
    step->insert = insert;
    step->offset = offset;
    step->text = g_strdup(text);
    step->group = group;
    g_queue_push_tail(&stack->undo, step);
    if (g_queue_get_length(&stack->undo) > UNDO_LIMIT) step_free(g_queue_pop_head(&stack->undo));
    stack->can_merge = fresh && insert && is_word(text);
}

static void on_insert(GtkTextBuffer* buffer, GtkTextIter* at, gchar* text, gint len, gpointer data) {
    (void)buffer;
    char* copy = g_strndup(text, (gsize)len);
    record(data, 1, gtk_text_iter_get_offset(at), copy);
    g_free(copy);
}

static void on_delete(GtkTextBuffer* buffer, GtkTextIter* start, GtkTextIter* end, gpointer data) {
    char* text = gtk_text_buffer_get_text(buffer, start, end, TRUE);
    record(data, 0, gtk_text_iter_get_offset(start), text);
    g_free(text);
}

static void on_begin_action(GtkTextBuffer* buffer, gpointer data) {
    (void)buffer;
    UndoStack* stack = data;
    if (stack->action_depth++ == 0) stack->action_group = 0;
}

static void on_end_action(GtkTextBuffer* buffer, gpointer data) {
    (void)buffer;
    UndoStack* stack = data;
    if (stack->action_depth > 0) stack->action_depth--;
}

/* Puts back a step (`inverse` set) or takes it out again, and leaves the cursor where it went. */
static void apply(GtkTextBuffer* buffer, const UndoStep* step, int inverse) {
    GtkTextIter start;
    int length = (int)g_utf8_strlen(step->text, -1);
    int inserting = step->insert != inverse;
    gtk_text_buffer_get_iter_at_offset(buffer, &start, step->offset);
    if (inserting) {
        gtk_text_buffer_insert(buffer, &start, step->text, -1);
    } else {
        GtkTextIter end;
        gtk_text_buffer_get_iter_at_offset(buffer, &end, step->offset + length);
        gtk_text_buffer_delete(buffer, &start, &end);
    }
    gtk_text_buffer_get_iter_at_offset(buffer, &start, step->offset + (inserting ? length : 0));
    gtk_text_buffer_place_cursor(buffer, &start);
}

/* Moves the newest group from one history to the other. */
static int move_group(GtkTextBuffer* buffer, UndoStack* stack, int redo) {
    GQueue* from = redo ? &stack->redo : &stack->undo;
    GQueue* to = redo ? &stack->undo : &stack->redo;
    UndoStep* step = g_queue_peek_tail(from);
    if (!step) return 0;
    int group = step->group;

    stack->applying = 1;
    while ((step = g_queue_peek_tail(from)) && step->group == group) {
        g_queue_pop_tail(from);
        apply(buffer, step, !redo);
        g_queue_push_tail(to, step);
    }
    stack->applying = 0;
    stack->can_merge = 0;
    return 1;
}

void ng_linux_undo_attach(void* text_buffer) {
    if (!GTK_IS_TEXT_BUFFER(text_buffer)) return;
    UndoStack* stack = ng_control_attach(text_buffer, &undo_kind, sizeof(UndoStack), stack_free);
    stack->enabled = 1;
    g_signal_connect(text_buffer, "insert-text", G_CALLBACK(on_insert), stack);
    g_signal_connect(text_buffer, "delete-range", G_CALLBACK(on_delete), stack);
    g_signal_connect(text_buffer, "begin-user-action", G_CALLBACK(on_begin_action), stack);
    g_signal_connect(text_buffer, "end-user-action", G_CALLBACK(on_end_action), stack);
}

int ng_linux_undo(void* text_buffer, int redo) {
    UndoStack* stack = stack_of(text_buffer);
    return stack && stack->enabled && move_group(GTK_TEXT_BUFFER(text_buffer), stack, redo);
}

int ng_linux_can_undo(void* text_buffer, int redo) {
    UndoStack* stack = stack_of(text_buffer);
    if (!stack || !stack->enabled) return 0;
    return !g_queue_is_empty(redo ? &stack->redo : &stack->undo);
}

void ng_linux_undo_clear(void* text_buffer) {
    UndoStack* stack = stack_of(text_buffer);
    if (!stack) return;
    clear_steps(&stack->undo);
    clear_steps(&stack->redo);
    stack->can_merge = 0;
}

int ng_linux_undo_enable(void* text_buffer, int on) {
    UndoStack* stack = stack_of(text_buffer);
    if (!stack) return 0;
    stack->enabled = on ? 1 : 0;
    if (!stack->enabled) ng_linux_undo_clear(text_buffer);
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
    if (!GTK_IS_TEXT_VIEW(text_view)) return;
    g_signal_connect(text_view, "key-press-event", G_CALLBACK(on_undo_keys), NULL);
}
