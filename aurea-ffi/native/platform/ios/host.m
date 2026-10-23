#import "host.h"
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#include <crt_externs.h>
#include <math.h>

/* One Aurea UI filling the screen: the app draws each frame into a pixel buffer, which becomes the
   view's layer contents. A display link asks for frames while the UI wants them. */

static NGIosHost g_host;

@interface AureaHostView : UIView
@property (strong) CADisplayLink* link;
@property (assign) BOOL dirty;
@end

@implementation AureaHostView

- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.multipleTouchEnabled = NO;
        self.layer.contentsGravity = kCAGravityTopLeft;
        self.layer.magnificationFilter = kCAFilterNearest;
        _dirty = YES;
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
    [super removeFromSuperview];
}

- (void)layoutSubviews {
    [super layoutSubviews];
    _dirty = YES;
}

- (void)safeAreaInsetsDidChange {
    [super safeAreaInsetsDidChange];
    _dirty = YES;
}

- (void)traitCollectionDidChange:(UITraitCollection*)previous {
    [super traitCollectionDidChange:previous];
    _dirty = YES;
}

- (void)tick:(CADisplayLink*)link {
    (void)link;
    if (!_dirty || !g_host.frame) return;
    CGFloat scale = self.window.screen.scale ?: UIScreen.mainScreen.scale;
    CGSize size = self.bounds.size;
    unsigned width = (unsigned)lround(size.width * scale);
    unsigned height = (unsigned)lround(size.height * scale);
    if (width == 0 || height == 0) return;
    UIEdgeInsets safe = self.safeAreaInsets;
    const uint32_t* words = NULL;
    unsigned out_w = 0, out_h = 0;
    _dirty = g_host.frame(width, height, (float)scale, (float)safe.left, (float)safe.top,
                          (float)safe.right, (float)safe.bottom, &words, &out_w, &out_h) != 0;
    if (!words || out_w == 0 || out_h == 0) return;

    /* The buffer is reused next frame, so the layer gets its own copy. */
    CFDataRef pixels = CFDataCreate(NULL, (const UInt8*)words, (CFIndex)out_w * out_h * 4);
    CGDataProviderRef data = CGDataProviderCreateWithCFData(pixels);
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGImageRef image = CGImageCreate(out_w, out_h, 8, 32, (size_t)out_w * 4, space,
                                     kCGBitmapByteOrder32Little | kCGImageAlphaNoneSkipFirst, data, NULL,
                                     false, kCGRenderingIntentDefault);
    if (image) {
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        self.layer.contentsScale = scale;
        self.layer.contents = (__bridge_transfer id)image;
        [CATransaction commit];
    }
    CGColorSpaceRelease(space);
    CGDataProviderRelease(data);
    CFRelease(pixels);
}

/* Lean of the pen in degrees from the screen's normal, as PenSample reads it: positive x toward
   the right, positive y toward the user. */
static void ng_ios_tilt(CGFloat altitude, CGFloat azimuth, float* tilt_x, float* tilt_y) {
    CGFloat flat = cos(altitude);
    CGFloat up = sin(altitude);
    *tilt_x = (float)(atan2(flat * cos(azimuth), up) * 180.0 / M_PI);
    *tilt_y = (float)(atan2(flat * sin(azimuth), up) * 180.0 / M_PI);
}

static double ng_ios_age(NSTimeInterval timestamp) {
    return NSProcessInfo.processInfo.systemUptime - timestamp;
}

- (void)pen:(int)phase touch:(UITouch*)touch {
    CGPoint at = [touch preciseLocationInView:self];
    CGFloat most = touch.maximumPossibleForce;
    float pressure = most > 0 ? (float)(touch.force / most) : 1.0f;
    float tilt_x = 0, tilt_y = 0, twist = 0;
    ng_ios_tilt(touch.altitudeAngle, [touch azimuthAngleInView:self], &tilt_x, &tilt_y);
    if (@available(iOS 17.5, *)) twist = (float)(touch.rollAngle * 180.0 / M_PI);
    g_host.pen(phase, at.x, at.y, pressure, tilt_x, tilt_y, twist, ng_ios_age(touch.timestamp));
}

/* An Apple Pencil also reports every sample between frames, with pressure and tilt. */
- (void)pens:(int)phase touch:(UITouch*)touch event:(UIEvent*)event {
    if (touch.type != UITouchTypePencil || !g_host.pen) return;
    NSArray<UITouch*>* samples = [event coalescedTouchesForTouch:touch] ?: @[ touch ];
    for (UITouch* sample in samples) {
        BOOL last = sample == samples.lastObject;
        int at = phase == NG_IOS_PEN_DOWN && sample != samples.firstObject ? NG_IOS_PEN_MOVE
               : phase == NG_IOS_PEN_UP && !last                         ? NG_IOS_PEN_MOVE
                                                                         : phase;
        [self pen:at touch:sample];
    }
}

- (void)send:(int)action touches:(NSSet<UITouch*>*)touches event:(UIEvent*)event {
    UITouch* touch = touches.anyObject;
    if (!touch) return;
    static const int pen_phase[] = {NG_IOS_PEN_DOWN, NG_IOS_PEN_UP, NG_IOS_PEN_MOVE, NG_IOS_PEN_UP};
    [self pens:pen_phase[action] touch:touch event:event];
    if (g_host.touch) {
        CGPoint at = [touch locationInView:self];
        g_host.touch(action, (float)at.x, (float)at.y);
    }
    _dirty = YES;
}

/* A pencil held over the screen, on iPads that sense it. */
- (void)hovered:(UIHoverGestureRecognizer*)hover API_AVAILABLE(ios(13.0)) {
    if (!g_host.pen) return;
    if (@available(iOS 16.4, *)) {
        if (hover.zOffset <= 0) return;
        if (hover.state != UIGestureRecognizerStateBegan && hover.state != UIGestureRecognizerStateChanged) return;
        CGPoint at = [hover locationInView:self];
        float tilt_x = 0, tilt_y = 0, twist = 0;
        ng_ios_tilt(hover.altitudeAngle, [hover azimuthAngleInView:self], &tilt_x, &tilt_y);
        if (@available(iOS 17.5, *)) twist = (float)(hover.rollAngle * 180.0 / M_PI);
        g_host.pen(NG_IOS_PEN_HOVER, at.x, at.y, 0.0f, tilt_x, tilt_y, twist, 0.0);
        _dirty = YES;
    }
}

- (void)touchesBegan:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
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
    if (!g_host.light_theme) return UIStatusBarStyleDefault;
    if (@available(iOS 13.0, *)) {
        return g_host.light_theme() ? UIStatusBarStyleDarkContent : UIStatusBarStyleLightContent;
    }
    return UIStatusBarStyleDefault;
}

@end

static UIWindow* ng_ios_host_window(UIWindow* window) {
    window.rootViewController = [[AureaHostController alloc] init];
    [window makeKeyAndVisible];
    return window;
}

/* iOS 13 and later show the host through a scene; newer releases stop apps that don't. The app's
   Info.plist needs a UIApplicationSceneManifest for UIKit to ask for one. */
API_AVAILABLE(ios(13.0))
@interface AureaSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property (nonatomic, strong) UIWindow* window;
@end

@implementation AureaSceneDelegate

- (void)scene:(UIScene*)scene willConnectToSession:(UISceneSession*)session options:(UISceneConnectionOptions*)options {
    (void)session;
    (void)options;
    if (![scene isKindOfClass:[UIWindowScene class]]) return;
    self.window = ng_ios_host_window([[UIWindow alloc] initWithWindowScene:(UIWindowScene*)scene]);
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
    g_host = host;
    @autoreleasepool {
        return UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), nil, NSStringFromClass([AureaHostDelegate class]));
    }
}
