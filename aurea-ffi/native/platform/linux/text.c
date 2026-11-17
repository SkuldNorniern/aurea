#include "text.h"
#include "ime.h"
#include "common/rust_callbacks.h"

void ng_linux_text_key(GtkWidget* widget, GdkEventKey* event) {
    if (!ng_linux_ime_active(widget) && event->string && event->string[0] != '\0') {
        ng_invoke_text_input((void*)widget, event->string);
    }
}
