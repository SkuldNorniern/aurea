#include "utils.h"
#include "common/errors.h"
#include <gtk/gtk.h>

static gboolean gtk_initialized = FALSE;

int ng_linux_init(void) {
    if (!gtk_initialized) {
        int argc = 0;
        char **argv = NULL;
        gtk_init(&argc, &argv);
        gtk_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_linux_cleanup(void) {
    if (gtk_initialized) {
        gtk_initialized = FALSE;
    }
}

gboolean ng_linux_is_initialized(void) {
    return gtk_initialized;
}

