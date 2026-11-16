#ifndef NATIVE_GUI_LINUX_CONTROL_H
#define NATIVE_GUI_LINUX_CONTROL_H

#include <gtk/gtk.h>
#include <stddef.h>

/* The state every control keeps. A control's own state struct starts with this, so one pointer
   serves both; the struct is attached with the control and freed with it. */
typedef struct {
    const void* kind;
    unsigned int id;
    gboolean watching;
    gboolean quiet;
} NgControl;

/* Attaches a zeroed `size`-byte state of `kind` to `object`; `destroy` frees it with `object`.
   `kind` is the address of a static tag unique to the state's type. */
void* ng_control_attach(gpointer object, const void* kind, size_t size, GDestroyNotify destroy);

/* The control state on `object`, of any kind; one is made on first use. */
NgControl* ng_control_get(gpointer object);

/* The control state on `object` when it is of `kind`, else NULL. */
void* ng_control_typed(gpointer object, const void* kind);

/* Reports `signal` on `object` as change `id`. Connects once, so later calls only set the id. */
void ng_control_watch(gpointer object, const char* signal, unsigned int id);

/* Takes `widget` out of its parent so it can be packed elsewhere. A list row that only held it
   goes too. Whoever calls this must hold a reference, or the removal may free the widget. */
void ng_linux_unparent(GtkWidget* widget);

#endif
