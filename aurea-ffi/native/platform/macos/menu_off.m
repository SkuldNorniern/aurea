#import <Cocoa/Cocoa.h>
#import "menu.h"

/* Without menus there is no bar and no shortcut of one. */
void ng_macos_menu_bar_install(void) {}

BOOL ng_macos_menu_key(NSEvent* event) {
    (void)event;
    return NO;
}
