#import "ime.h"
#import "window.h"
#import "common/errors.h"

/* Without IME the composition stays with the input method, the app only gets the result. */
void ng_macos_ime_preedit(AureaContentView* view, NSString* text, NSRange selected) {
    (void)view;
    (void)text;
    (void)selected;
}

int ng_macos_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    (void)window;
    (void)x;
    (void)y;
    (void)width;
    (void)height;
    return NG_SUCCESS;
}
