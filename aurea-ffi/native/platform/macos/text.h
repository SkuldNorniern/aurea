#ifndef NATIVE_GUI_MACOS_TEXT_H
#define NATIVE_GUI_MACOS_TEXT_H

#import <Cocoa/Cocoa.h>

@class AureaContentView;

/* YES while an input method is composing in `view`. */
BOOL ng_macos_text_composing(AureaContentView* view);
/* Gives a key press to the input method and the text it makes to the app. YES when the input
   method sent the key on as a command, for the key to go out after the text. */
BOOL ng_macos_text_key(AureaContentView* view, NSEvent* event);
/* A press commits what is being composed before it goes out. */
void ng_macos_text_commit(AureaContentView* view, NSEvent* event);
/* Focus is gone, the composition with it. */
void ng_macos_text_drop(AureaContentView* view);

#endif
