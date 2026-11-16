#include "control.h"
#include "elements.h"
#include "common/rust_callbacks.h"

#define NG_CONTROL_KEY "aurea-control"

void* ng_control_attach(gpointer object, const void* kind, size_t size, GDestroyNotify destroy) {
    NgControl* state = g_malloc0(size);
    state->kind = kind;
    g_object_set_data_full(G_OBJECT(object), NG_CONTROL_KEY, state, destroy);
    return state;
}

NgControl* ng_control_get(gpointer object) {
    if (!object) return NULL;
    NgControl* control = g_object_get_data(G_OBJECT(object), NG_CONTROL_KEY);
    if (!control) control = ng_control_attach(object, NULL, sizeof(NgControl), g_free);
    return control;
}

void* ng_control_typed(gpointer object, const void* kind) {
    if (!object || !kind) return NULL;
    NgControl* control = g_object_get_data(G_OBJECT(object), NG_CONTROL_KEY);
    return control && control->kind == kind ? control : NULL;
}

static void on_change(gpointer source, gpointer user_data) {
    (void)source;
    NgControl* control = user_data;
    if (!control->quiet) ng_invoke_custom_callback(control->id);
}

void ng_control_watch(gpointer object, const char* signal, unsigned int id) {
    NgControl* control = ng_control_get(object);
    if (!control) return;
    control->id = id;
    if (control->watching) return;
    control->watching = TRUE;
    g_signal_connect(object, signal, G_CALLBACK(on_change), control);
}

void ng_linux_unparent(GtkWidget* widget) {
    GtkWidget* parent = gtk_widget_get_parent(widget);
    if (!parent || !GTK_IS_CONTAINER(parent)) return;
    gtk_container_remove(GTK_CONTAINER(parent), widget);
    if (GTK_IS_LIST_BOX_ROW(parent)) gtk_widget_destroy(parent);
}

void ng_linux_set_quiet(void* object, int quiet) {
    NgControl* control = ng_control_get(object);
    if (control) control->quiet = quiet ? TRUE : FALSE;
}

int ng_linux_is_quiet(void* object) {
    NgControl* control = ng_control_get(object);
    return control && control->quiet;
}
