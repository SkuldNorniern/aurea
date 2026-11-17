#ifndef AUREA_WINDOWS_CLIPBOARD_H
#define AUREA_WINDOWS_CLIPBOARD_H

char* ng_windows_get_clipboard_text(void);
void ng_windows_free_clipboard_text(char* text);
int ng_windows_set_clipboard_text(const char* text);

#endif
