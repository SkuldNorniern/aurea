#ifndef NATIVE_GUI_MACOS_CONTENT_VIEW_H
#define NATIVE_GUI_MACOS_CONTENT_VIEW_H

#import <Cocoa/Cocoa.h>

/* The view every Aurea window holds. window.m has the shared part, the input feature files
   (pen, raw, gesture, drop, text, ime) take their events from it. */
@interface AureaContentView : NSView {
    NSTrackingArea* trackingArea;
    int buttonsHeld;
    BOOL exitHeld;
    unsigned int modifiersDown;
    NSString* markedText;
    BOOL imeSentCommand;
    NSMutableIndexSet* keysDown;
}
@property (nonatomic, assign) void* windowHandle;
@property (nonatomic, strong) NSCursor* cursor;
@property (nonatomic, assign) BOOL usesMetalLayer;
@property (nonatomic, assign) BOOL hidesCursor;
/* 0 free, 1 confined, 2 locked */
@property (nonatomic, assign) int grabMode;
/* the first raw delta after a lock is the warp to the middle, drop it */
@property (nonatomic, assign) BOOL rawSuppress;
/* the window's delegate: the window holds it only weakly, its view keeps it alive */
@property (nonatomic, strong) id<NSWindowDelegate> windowDelegate;
/* where the caret of the focused field is, for the candidate window. view points, top down */
@property (nonatomic, assign) NSRect imeArea;
@end

/* NG_MOD_* of an event. */
unsigned int ng_macos_modifiers(NSEvent* event);
/* The modifiers of a mouse event, with NG_POINTER_PEN when a tablet moved it. */
unsigned int ng_macos_mouse_modifiers(NSEvent* event);
/* NG_KEY_* of a hardware key code. */
unsigned int ng_macos_keycode_from_event(unsigned short keycode);

#endif
