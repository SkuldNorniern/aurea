#ifndef NATIVE_GUI_MACOS_H
#define NATIVE_GUI_MACOS_H

#include "../common/platform_api.h"

int ng_macos_init(void);
void ng_macos_cleanup(void);
int ng_macos_run(void);
int ng_macos_poll_events(void);
void ng_macos_request_frame(void);
void ng_macos_frame_idle(void);

#ifdef __OBJC__
@class NSWindow;
@class NSMenu;
@class NSView;
@class NSButton;
@class NSTextField;
@class NSStackView;
#endif

#endif // NATIVE_GUI_MACOS_H
