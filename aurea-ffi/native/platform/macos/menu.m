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

@interface NGEditMenu : NSMenu
@end

@implementation NGEditMenu
- (BOOL)performKeyEquivalent:(NSEvent*)event {
    const NSEventModifierFlags relevant = NSEventModifierFlagCommand | NSEventModifierFlagShift
        | NSEventModifierFlagOption | NSEventModifierFlagControl;
    NSEventModifierFlags mods = event.modifierFlags & relevant;
    NSString* key = [event.charactersIgnoringModifiers lowercaseString];
    for (NSMenuItem* item in self.itemArray) {
        if (!item.action || item.keyEquivalent.length == 0) continue;
        if (![[item.keyEquivalent lowercaseString] isEqualToString:key]) continue;
        if ((item.keyEquivalentModifierMask & relevant) != mods) continue;
        id handler = item.target ?: [NSApp targetForAction:item.action to:nil from:item];
        if (!handler || ([handler respondsToSelector:@selector(validateMenuItem:)]
                         && ![handler validateMenuItem:item])) {
            return NO;
        }
        return [super performKeyEquivalent:event];
    }
    return [super performKeyEquivalent:event];
}
@end

static NSMenuItem* add_standard(NSMenu* menu, NSString* title, SEL action, NSString* key, NSEventModifierFlags mods) {
    NSMenuItem* item = [menu addItemWithTitle:title action:action keyEquivalent:key];
    [item setKeyEquivalentModifierMask:mods];
    return item;
}

static NSMenuItem* top_level(NSMenu* bar, NSString* title, NSMenu** out) {
    NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:title action:nil keyEquivalent:@""];
    Class kind = [title isEqualToString:@"Edit"] ? [NGEditMenu class] : [NSMenu class];
    NSMenu* menu = [[kind alloc] initWithTitle:title];
    [item setSubmenu:menu];
    [bar addItem:item];
    *out = menu;
    return item;
}

NSMenu* ng_macos_standard_menu_bar(void) {
    @autoreleasepool {
        NSMenu* bar = [[NSMenu alloc] init];
        NSString* appName = [[NSProcessInfo processInfo] processName];
        const NSEventModifierFlags cmd = NSEventModifierFlagCommand;

        NSMenu* app = nil;
        top_level(bar, appName, &app);
        add_standard(app, [NSString stringWithFormat:@"Hide %@", appName], @selector(hide:), @"h", cmd);
        add_standard(app, @"Hide Others", @selector(hideOtherApplications:), @"h", cmd | NSEventModifierFlagOption);
        add_standard(app, @"Show All", @selector(unhideAllApplications:), @"", 0);
        [app addItem:[NSMenuItem separatorItem]];
        add_standard(app, [NSString stringWithFormat:@"Quit %@", appName], @selector(terminate:), @"q", cmd);

        NSMenu* edit = nil;
        top_level(bar, @"Edit", &edit);
        add_standard(edit, @"Undo", @selector(undo:), @"z", cmd);
        add_standard(edit, @"Redo", @selector(redo:), @"z", cmd | NSEventModifierFlagShift);
        [edit addItem:[NSMenuItem separatorItem]];
        add_standard(edit, @"Cut", @selector(cut:), @"x", cmd);
        add_standard(edit, @"Copy", @selector(copy:), @"c", cmd);
        add_standard(edit, @"Paste", @selector(paste:), @"v", cmd);
        add_standard(edit, @"Select All", @selector(selectAll:), @"a", cmd);

        NSMenu* window = nil;
        top_level(bar, @"Window", &window);
        add_standard(window, @"Minimize", @selector(performMiniaturize:), @"m", cmd);
        add_standard(window, @"Zoom", @selector(performZoom:), @"", 0);
        add_standard(window, @"Close", @selector(performClose:), @"w", cmd);
        [window addItem:[NSMenuItem separatorItem]];
        add_standard(window, @"Bring All to Front", @selector(arrangeInFront:), @"", 0);
        [NSApp setWindowsMenu:window];

        return bar;
    }
}

NGMenuHandle ng_macos_create_menu(void) {
    if (!menuItemTarget) {
        menuItemTarget = [[MenuItemTarget alloc] init];
    }
    return (__bridge_retained void*)ng_macos_standard_menu_bar();
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

static NSString* strip_access_keys(NSString* title) {
    NSMutableString* out = [NSMutableString stringWithCapacity:title.length];
    NSUInteger n = title.length;
    for (NSUInteger i = 0; i < n; i++) {
        unichar c = [title characterAtIndex:i];
        if (c != '&') {
            [out appendFormat:@"%C", c];
        } else if (i + 1 < n && [title characterAtIndex:i + 1] == '&') {
            [out appendString:@"&"];
            i++;
        }
    }
    return out;
}

static NSString* key_named(NSString* name) {
    if (name.length == 1) return [name lowercaseString];
    if (name.length >= 2 && name.length <= 3 && [name characterAtIndex:0] == 'F') {
        NSInteger n = [[name substringFromIndex:1] integerValue];
        if (n >= 1 && n <= 35) {
            unichar f = (unichar)(NSF1FunctionKey + n - 1);
            return [NSString stringWithCharacters:&f length:1];
        }
    }
    static NSDictionary<NSString*, NSNumber*>* names = nil;
    if (!names) {
        names = @{
            @"enter": @('\r'), @"return": @('\r'), @"tab": @('\t'), @"escape": @(0x1B),
            @"esc": @(0x1B), @"space": @(' '), @"backspace": @(NSBackspaceCharacter),
            @"delete": @(NSDeleteFunctionKey), @"plus": @('+'),
        };
    }
    NSNumber* code = names[[name lowercaseString]];
    if (!code) return nil;
    unichar c = (unichar)code.unsignedShortValue;
    return [NSString stringWithCharacters:&c length:1];
}

static void parse_title_and_shortcut(NSString* full, NSString** outTitle, NSString** outKey, NSEventModifierFlags* outMod) {
    NSArray* parts = [full componentsSeparatedByString:@"\t"];
    NSString* displayTitle = parts.count > 0 ? parts[0] : full;
    *outTitle = strip_access_keys([displayTitle stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]]);
    *outKey = @"";
    *outMod = 0;
    if (parts.count < 2) return;
    NSString* keyCombo = [parts[1] stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (keyCombo.length == 0) return;
    NSArray* tokens = [keyCombo componentsSeparatedByString:@"+"];
    NSString* keyChar = nil;
    NSEventModifierFlags mods = 0;
    for (NSString* t in tokens) {
        NSString* s = [t stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
        if (s.length == 0) continue;
        if ([s caseInsensitiveCompare:@"Ctrl"] == NSOrderedSame
            || [s caseInsensitiveCompare:@"Cmd"] == NSOrderedSame
            || [s caseInsensitiveCompare:@"Command"] == NSOrderedSame
            || [s caseInsensitiveCompare:@"Meta"] == NSOrderedSame) {
            mods |= NSEventModifierFlagCommand;
        } else if ([s caseInsensitiveCompare:@"Control"] == NSOrderedSame) {
            mods |= NSEventModifierFlagControl;
        } else if ([s caseInsensitiveCompare:@"Alt"] == NSOrderedSame || [s caseInsensitiveCompare:@"Option"] == NSOrderedSame) {
            mods |= NSEventModifierFlagOption;
        } else if ([s caseInsensitiveCompare:@"Shift"] == NSOrderedSame) {
            mods |= NSEventModifierFlagShift;
        } else {
            keyChar = key_named(s);
        }
    }
    if ([keyCombo hasSuffix:@"++"] || [keyCombo isEqualToString:@"+"]) keyChar = @"+";
    if (keyChar.length > 0) {
        *outKey = keyChar;
        *outMod = mods;
    }
}

static void drop_standard_twin(NSMenu* menu, NSMenuItem* item) {
    for (NSMenuItem* other in [menu.itemArray copy]) {
        if (other.isSeparatorItem || other.submenu || other.action == @selector(menuItemClicked:)) continue;
        BOOL sameKey = item.keyEquivalent.length > 0
            && [other.keyEquivalent isEqualToString:item.keyEquivalent]
            && other.keyEquivalentModifierMask == item.keyEquivalentModifierMask;
        if (sameKey || [other.title isEqualToString:item.title]) [menu removeItem:other];
    }
    for (NSInteger i = menu.numberOfItems - 1; i >= 0; i--) {
        if (![menu itemAtIndex:i].isSeparatorItem) continue;
        if (i == 0 || [menu itemAtIndex:i - 1].isSeparatorItem) [menu removeItemAtIndex:i];
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

    [menuItem setKeyEquivalentModifierMask:modMask];

    [menuItem setTarget:menuItemTarget];
    [menuItem setTag:id];
    drop_standard_twin(parentMenu, menuItem);
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
    @autoreleasepool {
        if (!parentMenu || !title) return NULL;
    
        NSMenu* parent = (__bridge NSMenu*)parentMenu;
        NSString* itemTitle = strip_access_keys(ng_macos_to_nsstring(title));
    
        NSInteger window = parent.supermenu ? -1 : [parent indexOfItemWithTitle:@"Window"];
        BOOL isBar = window > 0;
        NSMenuItem* existing = isBar ? [parent itemWithTitle:itemTitle] : nil;
        if (existing.submenu && ([itemTitle isEqualToString:@"Edit"] || [itemTitle isEqualToString:@"Window"])) {
            [existing.submenu addItem:[NSMenuItem separatorItem]];
            return (__bridge void*)existing.submenu;
        }

        NSMenuItem* menuItem = [[NSMenuItem alloc] init];
        NSMenu* submenu = [[NSMenu alloc] initWithTitle:itemTitle];
    
        [menuItem setTitle:itemTitle];
        [menuItem setSubmenu:submenu];
        /* In the bar, File goes straight after the app menu, Help last and
           the rest before Window, the order Mac apps keep. */
        if (isBar && [itemTitle isEqualToString:@"File"]) {
            [parent insertItem:menuItem atIndex:1];
        } else if (isBar && ![itemTitle isEqualToString:@"Help"]) {
            [parent insertItem:menuItem atIndex:window];
        } else {
            [parent addItem:menuItem];
        }
    
        return (__bridge void*)submenu;
    }
}

static NSMenuItem* item_with_id(NGMenuHandle menu, unsigned int id) {
    if (!menu) return nil;
    return [(__bridge NSMenu*)menu itemWithTag:(NSInteger)id];
}

int ng_macos_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio) {
    (void)radio;
    int result = ng_macos_add_menu_item(menu, title, id);
    if (result != NG_SUCCESS) return result;
    NSMenuItem* item = item_with_id(menu, id);
    [item setState:checked ? NSControlStateValueOn : NSControlStateValueOff];
    return NG_SUCCESS;
}

int ng_macos_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled) {
    NSMenuItem* item = item_with_id(menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    [(__bridge NSMenu*)menu setAutoenablesItems:NO];
    [item setEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

int ng_macos_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked) {
    NSMenuItem* item = item_with_id(menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    [item setState:checked ? NSControlStateValueOn : NSControlStateValueOff];
    return NG_SUCCESS;
}

int ng_macos_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title) {
    NSMenuItem* item = item_with_id(menu, id);
    if (!item || !title) return NG_ERROR_INVALID_PARAMETER;
    NSString* displayTitle = nil;
    NSString* keyEquiv = nil;
    NSEventModifierFlags modMask = 0;
    parse_title_and_shortcut(ng_macos_to_nsstring(title), &displayTitle, &keyEquiv, &modMask);
    [item setTitle:displayTitle];
    [item setKeyEquivalent:keyEquiv ? keyEquiv : @""];
    [item setKeyEquivalentModifierMask:modMask];
    return NG_SUCCESS;
}

int ng_macos_menu_item_remove(NGMenuHandle menu, unsigned int id) {
    NSMenuItem* item = item_with_id(menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    [(__bridge NSMenu*)menu removeItem:item];
    return NG_SUCCESS;
}

int ng_macos_menu_clear(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    [(__bridge NSMenu*)menu removeAllItems];
    return NG_SUCCESS;
}
