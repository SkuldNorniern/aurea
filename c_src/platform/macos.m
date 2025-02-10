#import "macos.h"
#import "../common/errors.h"
#import <Cocoa/Cocoa.h>

static BOOL app_initialized = FALSE;

@interface MenuItemTarget : NSObject
- (void)menuItemClicked:(id)sender;
@end

@implementation MenuItemTarget
- (void)menuItemClicked:(id)sender {
    NSMenuItem* item = (NSMenuItem*)sender;
    NSLog(@"Menu item clicked: %ld", [item tag]);
}
@end

static MenuItemTarget* menuItemTarget = nil;

static inline NSString* to_nsstring(const char* str) {
    return str ? [NSString stringWithUTF8String:str] : nil;
}

int ng_platform_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        [NSApp finishLaunching];
        
        menuItemTarget = [[MenuItemTarget alloc] init];
        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_platform_cleanup(void) {
    if (app_initialized) {
        menuItemTarget = nil;
        app_initialized = FALSE;
    }
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    if (!title) return NULL;
    
    NSString* windowTitle = to_nsstring(title);
    NSRect frame = NSMakeRect(0, 0, width, height);
    
    NSWindow* window = [[NSWindow alloc] 
        initWithContentRect:frame
        styleMask:NSWindowStyleMaskTitled |
                 NSWindowStyleMaskClosable |
                 NSWindowStyleMaskMiniaturizable |
                 NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered
        defer:NO];
    
    [window setTitle:windowTitle];
    [window center];
    [window makeKeyAndOrderFront:nil];
    
    return (__bridge_retained void*)window;
}

void ng_platform_destroy_window(NGHandle handle) {
    if (!handle) return;
    NSWindow* window = (__bridge_transfer NSWindow*)handle;
    [window close];
}

NGMenuHandle ng_platform_create_menu(void) {
    // Create the main menu bar
    NSMenu* mainMenu = [[NSMenu alloc] init];
    
    // Create the application menu
    NSMenuItem* appMenuItem = [[NSMenuItem alloc] init];
    NSMenu* appMenu = [[NSMenu alloc] init];
    NSString* appName = [[NSProcessInfo processInfo] processName];
    
    // Add Quit item
    NSMenuItem* quitMenuItem = [[NSMenuItem alloc] 
        initWithTitle:[NSString stringWithFormat:@"Quit %@", appName]
        action:@selector(terminate:)
        keyEquivalent:@"q"];
    [appMenu addItem:quitMenuItem];
    
    [appMenuItem setSubmenu:appMenu];
    [mainMenu addItem:appMenuItem];
    
    [NSApp setMainMenu:mainMenu];
    return (__bridge_retained void*)mainMenu;
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    NSMenu* menu = (__bridge_transfer NSMenu*)handle;
    (void)menu; // Silence unused variable warning
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;
    [NSApp setMainMenu:(__bridge NSMenu*)menu];
    return NG_SUCCESS;
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    
    NSMenu* mainMenu = (__bridge NSMenu*)menu;
    NSString* itemTitle = to_nsstring(title);
    
    // Create menu item and its submenu
    NSMenuItem* menuItem = [[NSMenuItem alloc] init];
    NSMenu* submenu = [[NSMenu alloc] initWithTitle:itemTitle];
    [menuItem setSubmenu:submenu];
    [menuItem setTitle:itemTitle];
    
    // Create a default item in the submenu
    NSMenuItem* defaultItem = [[NSMenuItem alloc] 
        initWithTitle:itemTitle
        action:@selector(menuItemClicked:)
        keyEquivalent:@""];
    
    [defaultItem setTarget:menuItemTarget];
    [defaultItem setTag:id];
    [submenu addItem:defaultItem];
    
    // Add the menu item to the main menu
    [mainMenu addItem:menuItem];
    
    return NG_SUCCESS;
}

int ng_platform_run(void) {
    [NSApp run];
    return NG_SUCCESS;
} 