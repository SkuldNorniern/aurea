#ifndef AUREA_WINDOWS_TEXT_H
#define AUREA_WINDOWS_TEXT_H

#include "window_state.h"

/* A WM_CHAR for `w`'s window; a surrogate pair is joined across calls. */
void ng_windows_text_char(NGWindow* w, wchar_t wc);

#endif
