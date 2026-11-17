#ifndef NATIVE_GUI_IOS_TEXT_H
#define NATIVE_GUI_IOS_TEXT_H

#import <UIKit/UIKit.h>

@class AureaHostView;

/* The hidden text field the keyboard types into, in `host`. */
void ng_ios_text_attach(AureaHostView* host);
/* A touch commits what is being composed. */
void ng_ios_text_commit(AureaHostView* host);

#endif
