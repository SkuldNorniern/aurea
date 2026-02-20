#import "menu.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>

@interface MenuItemTarget : NSObject
- (void)menuItemClicked:(id)sender;
@end

@implementation MenuItemTarget
- (void)menuItemClicked:(id)sender {
    NSMenuItem* item = (NSMenuItem*)sender;
    unsigned int id = (unsigned int)[item tag];
    ng_invoke_menu_callback(id);
}
@end

static MenuItemTarget* menuItemTarget = nil;

NGMenuHandle ng_macos_create_menu(void) {
    // Initialize the MenuItemTarget if not already done
    if (!menuItemTarget) {
        menuItemTarget = [[MenuItemTarget alloc] init];
    }

    NSMenu* mainMenu = [[NSMenu alloc] init];
    
    // Create the application menu (required for macOS)
    NSMenuItem* appMenuItem = [[NSMenuItem alloc] init];
    NSMenu* appMenu = [[NSMenu alloc] init];
    NSString* appName = [[NSProcessInfo processInfo] processName];
    
    // Add Quit item to application menu
    [appMenu addItemWithTitle:[NSString stringWithFormat:@"Quit %@", appName]
                      action:@selector(terminate:)
               keyEquivalent:@"q"];
    
    [appMenuItem setSubmenu:appMenu];
    [mainMenu addItem:appMenuItem];
    
    return (__bridge_retained void*)mainMenu;
}

void ng_macos_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    NSMenu* menu = (__bridge_transfer NSMenu*)handle;
    (void)menu;
}

int ng_macos_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    [NSApp setMainMenu:(__bridge NSMenu*)menu];
    return NG_SUCCESS;
}

/* Parse "Title\tMod+Key" into display title and shortcut.
 * Mod can be Ctrl, Cmd, Alt, Shift or combinations (Ctrl+Shift+S).
 * On macOS, Ctrl in label typically maps to Cmd for standard shortcuts. */
static void parse_title_and_shortcut(NSString* full, NSString** outTitle, NSString** outKey, NSEventModifierFlags* outMod) {
    NSArray* parts = [full componentsSeparatedByString:@"\t"];
    NSString* displayTitle = parts.count > 0 ? parts[0] : full;
    *outTitle = [displayTitle stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    *outKey = @"";
    *outMod = 0;
    if (parts.count < 2) return;
    NSString* keyCombo = [parts[1] stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (keyCombo.length == 0) return;
    NSArray* tokens = [keyCombo componentsSeparatedByString:@"+"];
    NSString* keyChar = nil;
    for (NSString* t in tokens) {
        NSString* s = [t stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
        if (s.length == 0) continue;
        if ([s caseInsensitiveCompare:@"Ctrl"] == NSOrderedSame) {
            *outMod |= NSEventModifierFlagCommand;
        } else if ([s caseInsensitiveCompare:@"Cmd"] == NSOrderedSame || [s caseInsensitiveCompare:@"Command"] == NSOrderedSame) {
            *outMod |= NSEventModifierFlagCommand;
        } else if ([s caseInsensitiveCompare:@"Alt"] == NSOrderedSame || [s caseInsensitiveCompare:@"Option"] == NSOrderedSame) {
            *outMod |= NSEventModifierFlagOption;
        } else if ([s caseInsensitiveCompare:@"Shift"] == NSOrderedSame) {
            *outMod |= NSEventModifierFlagShift;
        } else if (s.length == 1) {
            keyChar = [s lowercaseString];
        } else if (s.length > 1 && [s caseInsensitiveCompare:@"Alt"] != NSOrderedSame) {
            keyChar = [s substringToIndex:1];
            keyChar = [keyChar lowercaseString];
        }
    }
    if (keyChar && keyChar.length > 0) {
        *outKey = keyChar;
    }
}

int ng_macos_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;

    NSMenu* parentMenu = (__bridge NSMenu*)menu;
    NSString* fullTitle = ng_macos_to_nsstring(title);
    NSString* displayTitle = nil;
    NSString* keyEquiv = nil;
    NSEventModifierFlags modMask = 0;
    parse_title_and_shortcut(fullTitle, &displayTitle, &keyEquiv, &modMask);

    NSMenuItem* menuItem = [[NSMenuItem alloc]
        initWithTitle:displayTitle
        action:@selector(menuItemClicked:)
        keyEquivalent:keyEquiv ? keyEquiv : @""];

    if (modMask != 0) {
        [menuItem setKeyEquivalentModifierMask:modMask];
    }

    [menuItem setTarget:menuItemTarget];
    [menuItem setTag:id];
    [parentMenu addItem:menuItem];

    return NG_SUCCESS;
}

int ng_macos_add_menu_separator(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_PARAMETER;

    NSMenu* parentMenu = (__bridge NSMenu*)menu;
    [parentMenu addItem:[NSMenuItem separatorItem]];

    return NG_SUCCESS;
}

NGMenuHandle ng_macos_create_submenu(NGMenuHandle parentMenu, const char* title) {
    if (!parentMenu || !title) return NULL;
    
    NSMenu* parent = (__bridge NSMenu*)parentMenu;
    NSString* itemTitle = ng_macos_to_nsstring(title);
    
    NSMenuItem* menuItem = [[NSMenuItem alloc] init];
    NSMenu* submenu = [[NSMenu alloc] initWithTitle:itemTitle];
    
    [menuItem setTitle:itemTitle];
    [menuItem setSubmenu:submenu];
    [parent addItem:menuItem];
    
    return (__bridge_retained void*)submenu;
}
