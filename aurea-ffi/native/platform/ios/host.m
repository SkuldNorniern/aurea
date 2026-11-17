#include <os/proc.h>
#import "host.h"
#import "host_view.h"
#import "drop.h"
#import "touch.h"
#import "pen.h"
#import "text.h"
#import "ios.h"
#import "../../common/input.h"
#import "../../common/rust_callbacks.h"
#import <GameController/GameController.h>
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#include <crt_externs.h>
#include <math.h>
#include <string.h>

void ng_ios_end_background(void);
static void ng_ios_device_changed(NSNotification* note);

/* One Aurea UI filling the screen: the app draws each frame of its safe area into a pixel buffer,
   which becomes a sublayer's contents; the view's own colour fills the rest. A display link asks for frames while the UI wants them, and pauses when
   it has had nothing to do for a while. */

NGIosHost g_ios_host;

/* How many idle refreshes before the display link pauses: a quarter second at 120 Hz. */
#define NG_IOS_IDLE_TICKS 30

/* The side of a presented tile in pixels: a multiple of the rasterizer's 128 pixel tiles. */
#define NG_IOS_TILE 256u

__weak AureaHostView* g_ios_view;

@implementation AureaHostView

- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.multipleTouchEnabled = YES;
        _content = [CALayer layer];
        _tiles = [NSMutableArray array];
        [self.layer addSublayer:_content];
        _background = UINT32_MAX;
        _pointerButton = -1;
        _dirty = YES;
        g_ios_view = self;
        ng_ios_pen_attach(self);
        ng_ios_drop_attach(self);
        ng_ios_text_attach(self);
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(keyboardChanged:)
                                                     name:UIKeyboardWillChangeFrameNotification
                                                   object:nil];
        if (@available(iOS 14.0, *)) {
            [[NSNotificationCenter defaultCenter] addObserver:self
                                                     selector:@selector(hardKeyboardChanged:)
                                                         name:GCKeyboardDidConnectNotification
                                                       object:nil];
            [[NSNotificationCenter defaultCenter] addObserver:self
                                                     selector:@selector(hardKeyboardChanged:)
                                                         name:GCKeyboardDidDisconnectNotification
                                                       object:nil];
        }
        [self hardKeyboardChanged:nil];
        for (NSNotificationName name in @[
                 NSProcessInfoThermalStateDidChangeNotification,
                 NSProcessInfoPowerStateDidChangeNotification
             ]) {
            [[NSNotificationCenter defaultCenter] addObserverForName:name
                                                              object:nil
                                                               queue:NSOperationQueue.mainQueue
                                                          usingBlock:^(NSNotification* note) {
                                                              ng_ios_device_changed(note);
                                                          }];
        }
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(memoryWarning:)
                                                     name:UIApplicationDidReceiveMemoryWarningNotification
                                                   object:nil];
        if (@available(iOS 13.0, *)) {
            [self addGestureRecognizer:[[UIHoverGestureRecognizer alloc] initWithTarget:self action:@selector(hovered:)]];
        }
        _link = [CADisplayLink displayLinkWithTarget:self selector:@selector(tick:)];
        [_link addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
    }
    return self;
}

- (void)removeFromSuperview {
    [_link invalidate];
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [super removeFromSuperview];
}

- (void)wakeLink {
    _idle = 0;
    _link.paused = NO;
}

- (void)markDirty {
    _dirty = YES;
    [self wakeLink];
}

- (void)hardKeyboardChanged:(NSNotification*)note {
    (void)note;
    // no GCKeyboard before iOS 14, report no hardware keyboard
    if (@available(iOS 14.0, *)) {
        g_ios_host.keyboard(GCKeyboard.coalescedKeyboard != nil);
    } else {
        g_ios_host.keyboard(false);
    }
}

/* How much of the view the on-screen keyboard covers, for ng_ios_keyboard_height. */
- (void)keyboardChanged:(NSNotification*)note {
    CGRect end = [note.userInfo[UIKeyboardFrameEndUserInfoKey] CGRectValue];
    CGRect mine = [self convertRect:end fromCoordinateSpace:self.window.screen.coordinateSpace];
    _keyboardHeight = MAX(0, CGRectGetMaxY(self.bounds) - CGRectGetMinY(mine));
    [self markDirty];
}

- (void)memoryWarning:(NSNotification*)note {
    (void)note;
    if (g_ios_host.lifecycle) g_ios_host.lifecycle(NG_IOS_MEMORY_WARNING);
    [self markDirty];
}

- (void)layoutSubviews {
    [super layoutSubviews];
    [self markDirty];
}

- (void)safeAreaInsetsDidChange {
    [super safeAreaInsetsDidChange];
    [self markDirty];
}

- (void)traitCollectionDidChange:(UITraitCollection*)previous {
    [super traitCollectionDidChange:previous];
    [self markDirty];
}

/* The colour around the safe area follows the theme's background. */
- (void)paintBackground {
    if (!g_ios_host.background) return;
    uint32_t rgb = g_ios_host.background() & 0xFFFFFF;
    if (rgb == _background) return;
    _background = rgb;
    self.backgroundColor = [UIColor colorWithRed:((rgb >> 16) & 0xFF) / 255.0
                                           green:((rgb >> 8) & 0xFF) / 255.0
                                            blue:(rgb & 0xFF) / 255.0
                                           alpha:1.0];
}

- (void)tick:(CADisplayLink*)link {
    if (!g_ios_host.frame) return;
    /* `wake` is how other threads, and touches still being told apart, ask for a frame. */
    if (!_dirty && !(g_ios_host.wake && g_ios_host.wake())) {
        if (++_idle >= NG_IOS_IDLE_TICKS) link.paused = YES;
        return;
    }
    _idle = 0;
    CGFloat scale = self.window.screen.scale ?: UIScreen.mainScreen.scale;
    CGSize size = self.bounds.size;
    unsigned width = (unsigned)lround(size.width * scale);
    unsigned height = (unsigned)lround(size.height * scale);
    if (width == 0 || height == 0) return;
    UIEdgeInsets safe = self.safeAreaInsets;
    if (_gpuView) {
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        _gpuView.frame = UIEdgeInsetsInsetRect(self.bounds, safe);
        [CATransaction commit];
    }
    const uint32_t* words = NULL;
    unsigned out_w = 0, out_h = 0;
    unsigned damage[4] = {0, 0, 0, 0};
    _dirty = g_ios_host.frame(width, height, (float)scale, (float)safe.left, (float)safe.top, (float)safe.right,
                          (float)safe.bottom, &words, &out_w, &out_h, damage) != 0;
    [self paintBackground];
    if (!words || out_w == 0 || out_h == 0) return;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    _content.frame = CGRectMake(safe.left, safe.top, out_w / scale, out_h / scale);
    [self present:words width:out_w height:out_h damage:damage scale:scale];
    [CATransaction commit];
}

/* The frame goes up in tiles, and only the tiles it changed get new pictures: a brush dab costs
   its own few tiles rather than the whole screen. */
- (void)present:(const uint32_t*)words
          width:(unsigned)w
         height:(unsigned)h
         damage:(const unsigned*)damage
          scale:(CGFloat)scale {
    unsigned cols = (w + NG_IOS_TILE - 1) / NG_IOS_TILE;
    unsigned rows = (h + NG_IOS_TILE - 1) / NG_IOS_TILE;
    unsigned dx = damage[0], dy = damage[1], dw = damage[2], dh = damage[3];
    if (w != _gridWidth || h != _gridHeight || fabs(scale - _gridScale) > 0.001) {
        for (CALayer* tile in _tiles) [tile removeFromSuperlayer];
        [_tiles removeAllObjects];
        for (unsigned r = 0; r < rows; r++) {
            for (unsigned c = 0; c < cols; c++) {
                CALayer* tile = [CALayer layer];
                unsigned tw = MIN(NG_IOS_TILE, w - c * NG_IOS_TILE), th = MIN(NG_IOS_TILE, h - r * NG_IOS_TILE);
                tile.frame = CGRectMake(c * NG_IOS_TILE / scale, r * NG_IOS_TILE / scale, tw / scale, th / scale);
                tile.contentsScale = scale;
                tile.contentsGravity = kCAGravityTopLeft;
                tile.magnificationFilter = kCAFilterNearest;
                [_content addSublayer:tile];
                [_tiles addObject:tile];
            }
        }
        _gridWidth = w;
        _gridHeight = h;
        _gridScale = scale;
        dx = 0, dy = 0, dw = w, dh = h;
    }
    if (dw == 0 || dh == 0) return;
    static CGColorSpaceRef space = NULL;
    if (!space) space = CGColorSpaceCreateDeviceRGB(); // made once, the space never changes
    unsigned c0 = dx / NG_IOS_TILE, c1 = MIN(cols, (dx + dw + NG_IOS_TILE - 1) / NG_IOS_TILE);
    unsigned r0 = dy / NG_IOS_TILE, r1 = MIN(rows, (dy + dh + NG_IOS_TILE - 1) / NG_IOS_TILE);
    for (unsigned r = r0; r < r1; r++) {
        for (unsigned c = c0; c < c1; c++) {
            unsigned x = c * NG_IOS_TILE, y = r * NG_IOS_TILE;
            unsigned tw = MIN(NG_IOS_TILE, w - x), th = MIN(NG_IOS_TILE, h - y);
            /* The buffer is reused next frame, so the tile gets its own copy of its rows. */
            CFMutableDataRef pixels = CFDataCreateMutable(NULL, (CFIndex)tw * th * 4);
            CFDataSetLength(pixels, (CFIndex)tw * th * 4);
            UInt8* to = CFDataGetMutableBytePtr(pixels);
            for (unsigned row = 0; row < th; row++) {
                memcpy(to + (size_t)row * tw * 4, words + (size_t)(y + row) * w + x, (size_t)tw * 4);
            }
            CGDataProviderRef data = CGDataProviderCreateWithCFData(pixels);
            CGImageRef image = CGImageCreate(tw, th, 8, 32, (size_t)tw * 4, space,
                                             kCGBitmapByteOrder32Little | kCGImageAlphaNoneSkipFirst, data,
                                             NULL, false, kCGRenderingIntentDefault);
            if (image) _tiles[r * cols + c].contents = (__bridge_transfer id)image;
            CGDataProviderRelease(data);
            CFRelease(pixels);
        }
    }
}

/* A trackpad or mouse click comes as a touch of its own type, with the buttons on the event. The
   button is the one pressed when it began, so a drag keeps it. */
- (void)pointer:(int)action touch:(UITouch*)touch event:(UIEvent*)event API_AVAILABLE(ios(13.4)) {
    if (!g_ios_host.mouse) return;
    CGPoint at = [touch locationInView:self];
    if (action == NG_IOS_TOUCH_DOWN) {
        self.pointerButton = (event.buttonMask & UIEventButtonMaskSecondary) ? 1 : 0;
        g_ios_host.mouse(NG_IOS_MOUSE_PRESS, (float)at.x, (float)at.y, self.pointerButton);
    } else if (action == NG_IOS_TOUCH_UP || action == NG_IOS_TOUCH_CANCEL) {
        g_ios_host.mouse(NG_IOS_MOUSE_RELEASE, (float)at.x, (float)at.y, self.pointerButton);
        self.pointerButton = -1;
    } else {
        g_ios_host.mouse(NG_IOS_MOUSE_MOVE, (float)at.x, (float)at.y, self.pointerButton);
    }
}

- (void)send:(int)action touches:(NSSet<UITouch*>*)touches event:(UIEvent*)event {
    for (UITouch* touch in touches) {
        if (@available(iOS 13.4, *)) {
            if (touch.type == UITouchTypeIndirectPointer) {
                [self pointer:action touch:touch event:event];
                continue;
            }
        }
        ng_ios_pen_touch(self, action, touch, event);
        ng_ios_touch(self, action, touch);
    }
    [self markDirty];
}

/* A hardware keyboard: UIKit's HID usage codes as NG_KEY_* codes. */
static uint32_t ng_ios_key(long usage) {
    if (usage >= 0x04 && usage <= 0x1D) return NG_KEY_A + (uint32_t)(usage - 0x04);
    if (usage >= 0x1E && usage <= 0x26) return NG_KEY_1 + (uint32_t)(usage - 0x1E);
    if (usage >= 0x3A && usage <= 0x45) return NG_KEY_F1 + (uint32_t)(usage - 0x3A);
    switch (usage) {
        case 0x27: return NG_KEY_0;
        case 0x28: case 0x58: return NG_KEY_ENTER;
        case 0x29: return NG_KEY_ESCAPE;
        case 0x2A: return NG_KEY_BACKSPACE;
        case 0x2B: return NG_KEY_TAB;
        case 0x2C: return NG_KEY_SPACE;
        case 0x2D: return NG_KEY_MINUS;
        case 0x2E: return NG_KEY_EQUALS;
        case 0x2F: return NG_KEY_LEFT_BRACKET;
        case 0x30: return NG_KEY_RIGHT_BRACKET;
        case 0x31: return NG_KEY_BACKSLASH;
        case 0x33: return NG_KEY_SEMICOLON;
        case 0x34: return NG_KEY_APOSTROPHE;
        case 0x35: return NG_KEY_GRAVE;
        case 0x36: return NG_KEY_COMMA;
        case 0x37: return NG_KEY_PERIOD;
        case 0x38: return NG_KEY_SLASH;
        case 0x49: return NG_KEY_INSERT;
        case 0x4A: return NG_KEY_HOME;
        case 0x4B: return NG_KEY_PAGE_UP;
        case 0x4C: return NG_KEY_DELETE;
        case 0x4D: return NG_KEY_END;
        case 0x4E: return NG_KEY_PAGE_DOWN;
        case 0x4F: return NG_KEY_RIGHT;
        case 0x50: return NG_KEY_LEFT;
        case 0x51: return NG_KEY_DOWN;
        case 0x52: return NG_KEY_UP;
        case 0xE0: case 0xE4: return NG_KEY_CONTROL;
        case 0xE1: case 0xE5: return NG_KEY_SHIFT;
        case 0xE2: case 0xE6: return NG_KEY_ALT;
        case 0xE3: case 0xE7: return NG_KEY_META;
        default: return NG_KEY_UNKNOWN;
    }
}

static uint32_t ng_ios_modifiers(UIKeyModifierFlags flags) {
    uint32_t mods = 0;
    if (flags & UIKeyModifierShift) mods |= NG_MOD_SHIFT;
    if (flags & UIKeyModifierControl) mods |= NG_MOD_CTRL;
    if (flags & UIKeyModifierAlternate) mods |= NG_MOD_ALT;
    if (flags & UIKeyModifierCommand) mods |= NG_MOD_META;
    return mods;
}

- (BOOL)canBecomeFirstResponder {
    return YES;
}

- (void)didMoveToWindow {
    [super didMoveToWindow];
    if (self.window) [self becomeFirstResponder];
}

/* Returns the presses it did not take, for UIKit to pass on. */
- (NSSet<UIPress*>*)keys:(NSSet<UIPress*>*)presses pressed:(int)pressed {
    if (!g_ios_host.key) return presses;
    NSMutableSet<UIPress*>* rest = [NSMutableSet set];
    for (UIPress* press in presses) {
        if (@available(iOS 13.4, *)) {
            UIKey* key = press.key;
            if (!key) {
                [rest addObject:press];
                continue;
            }
            uint32_t mods = ng_ios_modifiers(key.modifierFlags);
            /* Text comes with presses alone, and not with shortcuts or control characters. */
            NSString* typed = pressed && !(mods & (NG_MOD_CTRL | NG_MOD_META)) ? key.characters : @"";
            if (typed.length == 1 && [typed characterAtIndex:0] < 0x20) typed = @"";
            g_ios_host.key(ng_ios_key(key.keyCode), pressed, mods, typed.UTF8String ?: "");
        } else {
            [rest addObject:press];
        }
    }
    if (presses.count != rest.count) [self markDirty];
    return rest;
}

- (void)pressesBegan:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [self keys:presses pressed:1];
    if (rest.count) [super pressesBegan:rest withEvent:event];
}

- (void)pressesEnded:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [self keys:presses pressed:0];
    if (rest.count) [super pressesEnded:rest withEvent:event];
}

- (void)pressesCancelled:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [self keys:presses pressed:0];
    if (rest.count) [super pressesCancelled:rest withEvent:event];
}

/* The menu shortcuts go to the app ahead of the keys, and show in the hold-Command overlay. */
- (NSArray<UIKeyCommand*>*)keyCommands {
    return ng_ios_key_commands();
}

- (void)keyCommand:(UIKeyCommand*)key {
    ng_ios_key_command(key);
}

/* A pencil held over the screen, on iPads that sense it, has a height above the screen; a trackpad
   or mouse pointer has none. Before iOS 16.4 the height is not known, so all of it is the pointer. */
- (void)hovered:(UIHoverGestureRecognizer*)hover API_AVAILABLE(ios(13.0)) {
    if (@available(iOS 16.4, *)) {
        if (hover.zOffset > 0) {
            ng_ios_pen_hover(self, hover);
            return;
        }
    }
    [self pointerHovered:hover];
}

/* A trackpad or mouse pointer over the view, moving with no button held. */
- (void)pointerHovered:(UIHoverGestureRecognizer*)hover {
    if (!g_ios_host.mouse) return;
    if (hover.state == UIGestureRecognizerStateEnded || hover.state == UIGestureRecognizerStateCancelled) {
        if (!self.pointerOver) return;
        self.pointerOver = NO;
        g_ios_host.mouse(NG_IOS_MOUSE_EXIT, 0, 0, -1);
        [self markDirty];
        return;
    }
    if (hover.state != UIGestureRecognizerStateBegan && hover.state != UIGestureRecognizerStateChanged) return;
    self.pointerOver = YES;
    CGPoint at = [hover locationInView:self];
    g_ios_host.mouse(NG_IOS_MOUSE_MOVE, (float)at.x, (float)at.y, -1);
    [self markDirty];
}

- (void)touchesBegan:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    ng_ios_text_commit(self);
    [self send:NG_IOS_TOUCH_DOWN touches:touches event:event];
}

- (void)touchesMoved:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    [self send:NG_IOS_TOUCH_MOVE touches:touches event:event];
}

- (void)touchesEnded:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    [self send:NG_IOS_TOUCH_UP touches:touches event:event];
}

- (void)touchesCancelled:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    [self send:NG_IOS_TOUCH_CANCEL touches:touches event:event];
}

@end

@interface AureaHostController : UIViewController
@end

@implementation AureaHostController

- (void)loadView {
    self.view = [[AureaHostView alloc] initWithFrame:UIScreen.mainScreen.bounds];
}

- (UIStatusBarStyle)preferredStatusBarStyle {
    if (!g_ios_host.light_theme) return UIStatusBarStyleDefault;
    if (@available(iOS 13.0, *)) {
        return g_ios_host.light_theme() ? UIStatusBarStyleDarkContent : UIStatusBarStyleLightContent;
    }
    return UIStatusBarStyleDefault;
}

@end

// UIBackgroundTaskInvalid is extern const, cant be a static initializer
static UIBackgroundTaskIdentifier g_background_task;
static BOOL g_background_active;

void ng_ios_end_background(void) {
    if (!g_background_active) return;
    g_background_active = NO;
    [UIApplication.sharedApplication endBackgroundTask:g_background_task];
    g_background_task = UIBackgroundTaskInvalid;
}

double ng_ios_background_time(void) {
    return UIApplication.sharedApplication.backgroundTimeRemaining;
}

void ng_apple_device_info(unsigned long long* total, unsigned long long* available, int* thermal,
                          int* low) {
    NSProcessInfo* process = NSProcessInfo.processInfo;
    *total = process.physicalMemory;
    *available = os_proc_available_memory();
    *thermal = (int)process.thermalState;
    *low = process.lowPowerModeEnabled ? 1 : 0;
}

static void ng_ios_device_changed(NSNotification* note) {
    (void)note;
    if (g_ios_host.lifecycle) g_ios_host.lifecycle(NG_IOS_DEVICE_CHANGED);
}

static void ng_ios_lifecycle(int state) {
    if (g_ios_host.lifecycle) g_ios_host.lifecycle(state);
    [g_ios_view markDirty];
}

static UIWindow* ng_ios_host_window(UIWindow* window) {
    window.rootViewController = [[AureaHostController alloc] init];
    [window makeKeyAndVisible];
    return window;
}

/* iOS 13 and later show the host through a scene; newer releases stop apps that don't. The app's
   Info.plist needs a UIApplicationSceneManifest for UIKit to ask for one. */
/* A document the system asked the app to open, copied in so its path stays the app's own. */
static void ng_ios_open(NSURL* url) {
    BOOL scoped = [url startAccessingSecurityScopedResource];
    NSString* folder = [NSTemporaryDirectory()
        stringByAppendingPathComponent:[NSString stringWithFormat:@"opened/%@", NSUUID.UUID.UUIDString]];
    NSString* path = [folder stringByAppendingPathComponent:url.lastPathComponent];
    NSFileManager* files = NSFileManager.defaultManager;
    BOOL copied = [files createDirectoryAtPath:folder withIntermediateDirectories:YES attributes:nil error:NULL]
        && [files copyItemAtURL:url toURL:[NSURL fileURLWithPath:path] error:NULL];
    if (scoped) [url stopAccessingSecurityScopedResource];
    if (copied) ng_invoke_file_opened(path.fileSystemRepresentation);
}

API_AVAILABLE(ios(13.0))
@interface AureaSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property (nonatomic, strong) UIWindow* window;
@end

@implementation AureaSceneDelegate

- (void)scene:(UIScene*)scene willConnectToSession:(UISceneSession*)session options:(UISceneConnectionOptions*)options {
    (void)session;
    if (![scene isKindOfClass:[UIWindowScene class]]) return;
    self.window = ng_ios_host_window([[UIWindow alloc] initWithWindowScene:(UIWindowScene*)scene]);
    for (UIOpenURLContext* context in options.URLContexts) ng_ios_open(context.URL);
}

- (void)scene:(UIScene*)scene openURLContexts:(NSSet<UIOpenURLContext*>*)contexts {
    (void)scene;
    for (UIOpenURLContext* context in contexts) ng_ios_open(context.URL);
}

- (void)sceneDidBecomeActive:(UIScene*)scene {
    (void)scene;
    ng_ios_lifecycle(NG_IOS_ACTIVE);
}

- (void)sceneWillResignActive:(UIScene*)scene {
    (void)scene;
    ng_ios_lifecycle(NG_IOS_INACTIVE);
}

- (void)sceneWillEnterForeground:(UIScene*)scene {
    (void)scene;
    ng_ios_lifecycle(NG_IOS_FOREGROUND);
}

- (void)sceneDidEnterBackground:(UIScene*)scene {
    (void)scene;
    ng_ios_end_background();
    g_background_task = [UIApplication.sharedApplication
        beginBackgroundTaskWithName:@"aurea.suspending"
                  expirationHandler:^{ ng_ios_end_background(); }];
    g_background_active = g_background_task != UIBackgroundTaskInvalid;
    ng_ios_lifecycle(NG_IOS_BACKGROUND);
    ng_ios_lifecycle(NG_IOS_SUSPENDING);
}

@end

@interface AureaHostDelegate : UIResponder <UIApplicationDelegate>
@property (nonatomic, strong) UIWindow* window;
@end

@implementation AureaHostDelegate

- (BOOL)application:(UIApplication*)application didFinishLaunchingWithOptions:(NSDictionary*)options {
    (void)application;
    (void)options;
    if (@available(iOS 13.0, *)) return YES;
    self.window = ng_ios_host_window([[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds]);
    return YES;
}

/* Before iOS 13; later ones come through the scene. */
- (BOOL)application:(UIApplication*)application openURL:(NSURL*)url options:(NSDictionary*)options {
    (void)application;
    (void)options;
    ng_ios_open(url);
    return YES;
}

/* The app's menus, as the menu bar on iPads with a keyboard; see ng_platform_create_menu. */
- (void)buildMenuWithBuilder:(id<UIMenuBuilder>)builder API_AVAILABLE(ios(13.0)) {
    ng_ios_build_menu(builder);
}

/* Before iOS 13 there are no scenes; the app itself reports its state. */
- (void)applicationDidBecomeActive:(UIApplication*)application {
    (void)application;
    if (@available(iOS 13.0, *)) return;
    ng_ios_lifecycle(NG_IOS_ACTIVE);
}

- (void)applicationWillResignActive:(UIApplication*)application {
    (void)application;
    if (@available(iOS 13.0, *)) return;
    ng_ios_lifecycle(NG_IOS_INACTIVE);
}

- (void)applicationWillEnterForeground:(UIApplication*)application {
    (void)application;
    if (@available(iOS 13.0, *)) return;
    ng_ios_lifecycle(NG_IOS_FOREGROUND);
}

- (void)applicationDidEnterBackground:(UIApplication*)application {
    (void)application;
    if (@available(iOS 13.0, *)) return;
    ng_ios_lifecycle(NG_IOS_BACKGROUND);
}

- (UISceneConfiguration*)application:(UIApplication*)application
    configurationForConnectingSceneSession:(UISceneSession*)session
                                   options:(UISceneConnectionOptions*)options API_AVAILABLE(ios(13.0)) {
    (void)application;
    (void)options;
    UISceneConfiguration* config = [[UISceneConfiguration alloc] initWithName:@"Aurea" sessionRole:session.role];
    config.delegateClass = [AureaSceneDelegate class];
    return config;
}

@end

int ng_ios_run_host(NGIosHost host) {
    g_ios_host = host;
    @autoreleasepool {
        return UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), nil, NSStringFromClass([AureaHostDelegate class]));
    }
}

void ng_ios_wake(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [g_ios_view wakeLink];
    });
}

void* ng_ios_gpu_view(void) {
    AureaHostView* host = g_ios_view;
    if (!host) return NULL;
    if (!host.gpuView) {
        UIView* view = [[UIView alloc] initWithFrame:UIEdgeInsetsInsetRect(host.bounds, host.safeAreaInsets)];
        view.userInteractionEnabled = NO;
        view.contentScaleFactor = host.window.screen.scale ?: UIScreen.mainScreen.scale;
        [host addSubview:view];
        host.gpuView = view;
    }
    return (__bridge void*)host.gpuView;
}

void ng_ios_drop_gpu_view(void) {
    AureaHostView* host = g_ios_view;
    [host.gpuView removeFromSuperview];
    host.gpuView = nil;
}

void ng_ios_show_native(void* view, float x, float y, float width, float height) {
    AureaHostView* host = g_ios_view;
    UIView* element = (__bridge UIView*)view;
    if (!host || !element) return;
    UIEdgeInsets safe = host.safeAreaInsets;
    element.translatesAutoresizingMaskIntoConstraints = YES;
    element.frame = CGRectMake(safe.left + x, safe.top + y, width, height);
    /* Widgets without a background would be unreadable over what the canvas draws. */
    if (!element.backgroundColor) element.backgroundColor = UIColor.systemBackgroundColor;
    if (element.superview != host) [host addSubview:element];
    [host bringSubviewToFront:element];
}

void ng_ios_hide_native(void* view) {
    [(__bridge UIView*)view removeFromSuperview];
}

float ng_ios_refresh_rate(void) {
    AureaHostView* host = g_ios_view;
    UIScreen* screen = host.window.screen ?: UIScreen.mainScreen;
    return (float)screen.maximumFramesPerSecond;
}

void ng_ios_set_high_frame_rate(int high) {
    AureaHostView* host = g_ios_view;
    if (!host) return;
    if (@available(iOS 15.0, *)) {
        float top = ng_ios_refresh_rate();
        host.link.preferredFrameRateRange =
            high && top > 60 ? CAFrameRateRangeMake(80, top, top) : CAFrameRateRangeDefault;
    }
}

