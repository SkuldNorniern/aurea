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
- (void)itemChosen:(NSMenuItem*)sender;
@end

@implementation AureaTray
- (void)clicked:(id)sender {
    NSEvent* event = [NSApp currentEvent];
    BOOL wantsMenu = event.type == NSEventTypeRightMouseUp
        || (event.modifierFlags & NSEventModifierFlagControl) != 0;
    if (wantsMenu) {
        if (self.menu.numberOfItems == 0) return;
        self.item.menu = self.menu;
        [self.item.button performClick:sender];
        self.item.menu = nil;
        return;
    }
    ng_invoke_menu_callback(self.clickId);
}

- (void)itemChosen:(NSMenuItem*)sender {
    ng_invoke_menu_callback((unsigned int)sender.tag);
}
@end

int ng_macos_tray_count = 0;

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
    NSImage* appIcon = [[NSApp applicationIconImage] copy];
    appIcon.size = NSMakeSize(TRAY_ICON_POINTS, TRAY_ICON_POINTS);
    button.image = appIcon;
    button.toolTip = tooltip ? ng_macos_to_nsstring(tooltip) : nil;
    button.target = tray;
    button.action = @selector(clicked:);
    [button sendActionOn:NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp];
    ng_macos_tray_count++;
    return (__bridge_retained void*)tray;
}

void ng_macos_destroy_tray(NGHandle handle) {
    if (!handle) return;
    AureaTray* tray = (__bridge_transfer AureaTray*)handle;
    tray.item.button.target = nil;
    [[NSStatusBar systemStatusBar] removeStatusItem:tray.item];
    ng_macos_tray_count--;
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
    if (![[NSBundle mainBundle] bundleIdentifier]) return NG_ERROR_PLATFORM_SPECIFIC;
    UNMutableNotificationContent* content = [[UNMutableNotificationContent alloc] init];
    content.title = ng_macos_to_nsstring(title);
    content.body = ng_macos_to_nsstring(body);
    UNNotificationRequest* request =
        [UNNotificationRequest requestWithIdentifier:[[NSUUID UUID] UUIDString]
                                             content:content
                                             trigger:nil];
    UNUserNotificationCenter* center = [UNUserNotificationCenter currentNotificationCenter];
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert | UNAuthorizationOptionSound
                          completionHandler:^(BOOL granted, NSError* error) {
        (void)error;
        if (granted) [center addNotificationRequest:request withCompletionHandler:nil];
    }];
    return NG_SUCCESS;
}
