#import "tray.h"
#import "utils.h"
#import "../../common/errors.h"
#import "../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <UserNotifications/UserNotifications.h>

/* An icon in the menu bar's status area, macOS's notification area. A left
   click runs the click callback; a right or Control click opens the menu,
   as on Windows. Menu items call back through the menu callback registry by
   their tag, like the menu bar's. */
@interface AureaTray : NSObject
@property (nonatomic, strong) NSStatusItem* item;
@property (nonatomic, strong) NSMenu* menu;
@property (nonatomic, assign) unsigned int clickId;
- (void)clicked:(id)sender;
- (BOOL)showMenu;
- (void)itemChosen:(NSMenuItem*)sender;
@end

@implementation AureaTray
- (void)clicked:(id)sender {
    NSEvent* event = [NSApp currentEvent];
    BOOL wantsMenu = event.type == NSEventTypeRightMouseUp
        || (event.modifierFlags & NSEventModifierFlagControl) != 0;
    if (wantsMenu) {
        [self showMenu];
        return;
    }
    ng_invoke_menu_callback(self.clickId);
}

/* Also VoiceOver's and keyboard users' way to the menu, through the
   button's "Show Menu" accessibility action; pressing it is the click. */
- (BOOL)showMenu {
    if (self.menu.numberOfItems == 0) return NO;
    /* Lent to the item only while it opens: kept there, the menu would
       take left clicks too and the click callback would never run. */
    self.item.menu = self.menu;
    [self.item.button performClick:nil];
    self.item.menu = nil;
    return YES;
}

- (void)itemChosen:(NSMenuItem*)sender {
    ng_invoke_menu_callback((unsigned int)sender.tag);
}
@end

/* Menu bar icons are 18 points tall; wider icons keep their shape. */
static const CGFloat TRAY_ICON_POINTS = 18.0;

NGHandle ng_macos_create_tray(const char* tooltip, unsigned int click_id) {
    AureaTray* tray = [[AureaTray alloc] init];
    tray.clickId = click_id;
    tray.menu = [[NSMenu alloc] init];
    tray.item = [[NSStatusBar systemStatusBar] statusItemWithLength:NSSquareStatusItemLength];
    NSStatusBarButton* button = tray.item.button;
    if (!button) {
        [[NSStatusBar systemStatusBar] removeStatusItem:tray.item];
        return NULL;
    }
    /* Until an icon is set, the app's own, as Windows shows its default. */
    NSImage* appIcon = [[NSApp applicationIconImage] copy];
    appIcon.size = NSMakeSize(TRAY_ICON_POINTS, TRAY_ICON_POINTS);
    button.image = appIcon;
    button.toolTip = tooltip ? ng_macos_to_nsstring(tooltip) : nil;
    button.target = tray;
    button.action = @selector(clicked:);
    [button sendActionOn:NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp];
    button.accessibilityCustomActions = @[[[NSAccessibilityCustomAction alloc]
        initWithName:@"Show Menu" target:tray selector:@selector(showMenu)]];
    return (__bridge_retained void*)tray;
}

void ng_macos_destroy_tray(NGHandle handle) {
    if (!handle) return;
    AureaTray* tray = (__bridge_transfer AureaTray*)handle;
    tray.item.button.target = nil;
    [[NSStatusBar systemStatusBar] removeStatusItem:tray.item];
}

int ng_macos_tray_set_icon_rgba(NGHandle handle, const unsigned char* rgba, unsigned int width, unsigned int height) {
    if (!handle || !rgba || width == 0 || height == 0) return NG_ERROR_INVALID_PARAMETER;
    AureaTray* tray = (__bridge AureaTray*)handle;
    NSBitmapImageRep* rep = [[NSBitmapImageRep alloc]
        initWithBitmapDataPlanes:NULL
                      pixelsWide:(NSInteger)width
                      pixelsHigh:(NSInteger)height
                   bitsPerSample:8
                 samplesPerPixel:4
                        hasAlpha:YES
                        isPlanar:NO
                  colorSpaceName:NSDeviceRGBColorSpace
                    bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
                     bytesPerRow:(NSInteger)width * 4
                    bitsPerPixel:32];
    if (!rep) return NG_ERROR_PLATFORM_SPECIFIC;
    memcpy([rep bitmapData], rgba, (size_t)width * height * 4);
    CGFloat w = TRAY_ICON_POINTS * (CGFloat)width / (CGFloat)height;
    NSImage* image = [[NSImage alloc] initWithSize:NSMakeSize(w, TRAY_ICON_POINTS)];
    [image addRepresentation:rep];
    tray.item.button.image = image;
    return NG_SUCCESS;
}

int ng_macos_tray_set_tooltip(NGHandle handle, const char* tooltip) {
    if (!handle || !tooltip) return NG_ERROR_INVALID_PARAMETER;
    AureaTray* tray = (__bridge AureaTray*)handle;
    tray.item.button.toolTip = ng_macos_to_nsstring(tooltip);
    return NG_SUCCESS;
}

int ng_macos_tray_add_item(NGHandle handle, const char* title, unsigned int id) {
    if (!handle) return NG_ERROR_INVALID_PARAMETER;
    AureaTray* tray = (__bridge AureaTray*)handle;
    if (!title) {
        [tray.menu addItem:[NSMenuItem separatorItem]];
        return NG_SUCCESS;
    }
    NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ng_macos_to_nsstring(title)
                                                  action:@selector(itemChosen:)
                                           keyEquivalent:@""];
    item.target = tray;
    item.tag = (NSInteger)id;
    [tray.menu addItem:item];
    return NG_SUCCESS;
}

int ng_macos_tray_notify(NGHandle handle, const char* title, const char* body) {
    if (!handle || !title || !body) return NG_ERROR_INVALID_PARAMETER;
    /* The notification center serves app bundles only; asked from a bare
       executable it throws. */
    if (![[NSBundle mainBundle] bundleIdentifier]) return NG_ERROR_PLATFORM_SPECIFIC;
    UNMutableNotificationContent* content = [[UNMutableNotificationContent alloc] init];
    content.title = ng_macos_to_nsstring(title);
    content.body = ng_macos_to_nsstring(body);
    UNNotificationRequest* request =
        [UNNotificationRequest requestWithIdentifier:[[NSUUID UUID] UUIDString]
                                             content:content
                                             trigger:nil];
    UNUserNotificationCenter* center = [UNUserNotificationCenter currentNotificationCenter];
    /* The first time, macOS asks the user; a refusal drops the
       notification, as Windows drops it with notifications turned off. */
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert | UNAuthorizationOptionSound
                          completionHandler:^(BOOL granted, NSError* error) {
        (void)error;
        if (granted) [center addNotificationRequest:request withCompletionHandler:nil];
    }];
    return NG_SUCCESS;
}
