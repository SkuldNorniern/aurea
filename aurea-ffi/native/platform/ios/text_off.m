#import "text.h"
#import "host.h"

/* Without text input there is no keyboard to show and nothing composes. */
void ng_ios_text_attach(AureaHostView* host) {
    (void)host;
}

void ng_ios_text_commit(AureaHostView* host) {
    (void)host;
}

void ng_ios_show_keyboard(int show) {
    (void)show;
}

float ng_ios_keyboard_height(void) {
    return 0.0f;
}

void ng_ios_set_text_area(float x, float y, float width, float height) {
    (void)x;
    (void)y;
    (void)width;
    (void)height;
}
