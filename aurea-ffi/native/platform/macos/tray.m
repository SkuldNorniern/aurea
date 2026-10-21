#import "tray.h"
#import "utils.h"
#import "../../common/errors.h"
#import "../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <UserNotifications/UserNotifications.h>

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

- (BOOL)showMenu {
    if (self.menu.numberOfItems == 0) return NO;
    self.item.menu = self.menu;
    [self.item.button performClick:nil];
    self.item.menu = nil;
    return YES;
}

- (void)itemChosen:(NSMenuItem*)sender {
    ng_invoke_menu_callback((unsigned int)sender.tag);
}
@end

static const CGFloat TRAY_ICON_POINTS = 18.0;

static BOOL rgba_is_monochrome(const unsigned char* rgba, size_t pixels) {
    BOOL any = NO;
    for (size_t i = 0; i < pixels; i++) {
        const unsigned char* p = rgba + i * 4;
        if (p[3] == 0) continue;
        if (p[0] > 24 || p[1] > 24 || p[2] > 24) return NO;
        any = YES;
    }
    return any;
}

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
    NSImage* appIcon = [[NSApp applicationIconImage] copy];
    appIcon.size = NSMakeSize(TRAY_ICON_POINTS, TRAY_ICON_POINTS);
    button.image = appIcon;
    button.toolTip = tooltip ? ng_macos_to_nsstring(tooltip) : nil;
    button.target = tray;
    button.action = @selector(clicked:);
    [button sendActionOn:NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp];
    if (@available(macOS 10.13, *)) {
        button.accessibilityCustomActions = @[[[NSAccessibilityCustomAction alloc]
            initWithName:@"Show Menu" target:tray selector:@selector(showMenu)]];
    }
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
    NSBitmapImageRep* rep = ng_macos_rgba_bitmap(rgba, width, height);
    if (!rep) return NG_ERROR_PLATFORM_SPECIFIC;
    CGFloat w = TRAY_ICON_POINTS * (CGFloat)width / (CGFloat)height;
    NSImage* image = [[NSImage alloc] initWithSize:NSMakeSize(w, TRAY_ICON_POINTS)];
    [image addRepresentation:rep];
    image.template = rgba_is_monochrome(rgba, (size_t)width * height);
    tray.item.button.image = image;
    tray.item.length = width == height ? NSSquareStatusItemLength : NSVariableStatusItemLength;
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

API_AVAILABLE(macos(10.14))
static int ng_macos_tray_notify_center(const char* title, const char* body) {
    UNMutableNotificationContent* content = [[UNMutableNotificationContent alloc] init];
    content.title = ng_macos_to_nsstring(title);
    content.body = ng_macos_to_nsstring(body);
    UNNotificationRequest* request =
        [UNNotificationRequest requestWithIdentifier:[[NSUUID UUID] UUIDString]
                                             content:content
                                             trigger:nil];
    UNUserNotificationCenter* center = [UNUserNotificationCenter currentNotificationCenter];

    dispatch_semaphore_t answered = dispatch_semaphore_create(0);
    __block UNAuthorizationStatus status = UNAuthorizationStatusNotDetermined;
    [center getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings* settings) {
        status = settings.authorizationStatus;
        dispatch_semaphore_signal(answered);
    }];
    BOOL known = dispatch_semaphore_wait(answered, dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC)) == 0;

    if (known && status == UNAuthorizationStatusDenied) return NG_ERROR_PLATFORM_SPECIFIC;
    if (known && status != UNAuthorizationStatusNotDetermined) {
        [center addNotificationRequest:request withCompletionHandler:nil];
        return NG_SUCCESS;
    }
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert | UNAuthorizationOptionSound
                          completionHandler:^(BOOL granted, NSError* error) {
        (void)error;
        if (granted) [center addNotificationRequest:request withCompletionHandler:nil];
    }];
    return NG_SUCCESS;
}

int ng_macos_tray_notify(NGHandle handle, const char* title, const char* body) {
    if (!handle || !title || !body) return NG_ERROR_INVALID_PARAMETER;
    if (![[NSBundle mainBundle] bundleIdentifier]) return NG_ERROR_PLATFORM_SPECIFIC;
    if (@available(macOS 10.14, *)) {
        return ng_macos_tray_notify_center(title, body);
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    NSUserNotification* note = [[NSUserNotification alloc] init];
    note.title = ng_macos_to_nsstring(title);
    note.informativeText = ng_macos_to_nsstring(body);
    [[NSUserNotificationCenter defaultUserNotificationCenter] deliverNotification:note];
#pragma clang diagnostic pop
    return NG_SUCCESS;
}
