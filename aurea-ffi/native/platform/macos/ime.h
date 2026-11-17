#ifndef NATIVE_GUI_MACOS_IME_H
#define NATIVE_GUI_MACOS_IME_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

/* Tells the app what is being composed, nil when it ended. `selected` is where the input method
   puts its caret in `text`. */
void ng_macos_ime_preedit(AureaContentView* view, NSString* text, NSRange selected);

#endif
