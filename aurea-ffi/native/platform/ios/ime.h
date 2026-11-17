#ifndef NATIVE_GUI_IOS_IME_H
#define NATIVE_GUI_IOS_IME_H

#import <Foundation/Foundation.h>

/* Tells the app what is being composed, `caret` in UTF-16 units of `marked`. */
void ng_ios_ime_preedit(NSString* marked, NSInteger caret);

#endif
