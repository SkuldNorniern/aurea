#import "ime.h"

/* Without IME the composition stays with the keyboard, the app only gets the result. */
void ng_ios_ime_preedit(NSString* marked, NSInteger caret) {
    (void)marked;
    (void)caret;
}
