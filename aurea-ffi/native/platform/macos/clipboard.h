#ifndef NATIVE_GUI_MACOS_CLIPBOARD_H
#define NATIVE_GUI_MACOS_CLIPBOARD_H

#ifdef __cplusplus
extern "C" {
#endif

char* ng_macos_get_clipboard_text(void);
void ng_macos_free_clipboard_text(char* text);
int ng_macos_set_clipboard_text(const char* text);

#ifdef __cplusplus
}
#endif

#endif
