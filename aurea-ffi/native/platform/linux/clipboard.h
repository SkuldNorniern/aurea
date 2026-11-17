#ifndef AUREA_LINUX_CLIPBOARD_H
#define AUREA_LINUX_CLIPBOARD_H

char* ng_linux_get_clipboard_text(void);
void ng_linux_free_clipboard_text(char* text);
int ng_linux_set_clipboard_text(const char* text);

#endif
