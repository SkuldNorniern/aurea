#import "host.h"
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#include <crt_externs.h>

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

- (void)send:(int)action touches:(NSSet<UITouch*>*)touches {
    UITouch* touch = touches.anyObject;
    if (!touch || !g_host.touch) return;
    CGPoint at = [touch locationInView:self];
    g_host.touch(action, (float)at.x, (float)at.y);
    _dirty = YES;
}

- (void)touchesBegan:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    (void)event;
    [self send:NG_IOS_TOUCH_DOWN touches:touches];
}

- (void)touchesMoved:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    (void)event;
    [self send:NG_IOS_TOUCH_MOVE touches:touches];
}

- (void)touchesEnded:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    (void)event;
    [self send:NG_IOS_TOUCH_UP touches:touches];
}

- (void)touchesCancelled:(NSSet<UITouch*>*)touches withEvent:(UIEvent*)event {
    (void)event;
    [self send:NG_IOS_TOUCH_CANCEL touches:touches];
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

@interface AureaHostDelegate : UIResponder <UIApplicationDelegate>
@property (nonatomic, strong) UIWindow* window;
@end

@implementation AureaHostDelegate

- (BOOL)application:(UIApplication*)application didFinishLaunchingWithOptions:(NSDictionary*)options {
    (void)application;
    (void)options;
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [[AureaHostController alloc] init];
    [self.window makeKeyAndVisible];
    return YES;
}

@end

int ng_ios_run_host(NGIosHost host) {
    g_host = host;
    @autoreleasepool {
        return UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), nil, NSStringFromClass([AureaHostDelegate class]));
    }
}
