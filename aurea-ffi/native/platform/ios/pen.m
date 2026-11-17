#import "pen.h"
#import "host_view.h"
#include <math.h>

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

static void ng_ios_pen_sample(UIView* view, int phase, UITouch* touch) {
    CGPoint at = [touch preciseLocationInView:view];
    CGFloat most = touch.maximumPossibleForce;
    float pressure = most > 0 ? (float)(touch.force / most) : 1.0f;
    float tilt_x = 0, tilt_y = 0, twist = 0;
    ng_ios_tilt(touch.altitudeAngle, [touch azimuthAngleInView:view], &tilt_x, &tilt_y);
    if (@available(iOS 17.5, *)) twist = (float)(touch.rollAngle * 180.0 / M_PI);
    g_ios_host.pen(phase, at.x, at.y, pressure, tilt_x, tilt_y, twist, ng_ios_age(touch.timestamp));
}

/* An Apple Pencil also reports every sample between frames, with pressure and tilt. */
void ng_ios_pen_touch(UIView* view, int action, UITouch* touch, UIEvent* event) {
    static const int pen_phase[] = {NG_IOS_PEN_DOWN, NG_IOS_PEN_UP, NG_IOS_PEN_MOVE, NG_IOS_PEN_CANCEL};
    if (touch.type != UITouchTypePencil || !g_ios_host.pen) return;
    int phase = pen_phase[action];
    NSArray<UITouch*>* samples = [event coalescedTouchesForTouch:touch] ?: @[ touch ];
    for (UITouch* sample in samples) {
        BOOL last = sample == samples.lastObject;
        int at = phase == NG_IOS_PEN_DOWN && sample != samples.firstObject ? NG_IOS_PEN_MOVE
               : (phase == NG_IOS_PEN_UP || phase == NG_IOS_PEN_CANCEL) && !last ? NG_IOS_PEN_MOVE
                                                                         : phase;
        ng_ios_pen_sample(view, at, sample);
    }
    /* Where it will likely go next, while it is down, for the stroke to seem to keep up. */
    if (phase == NG_IOS_PEN_DOWN || phase == NG_IOS_PEN_MOVE) {
        for (UITouch* ahead in [event predictedTouchesForTouch:touch]) {
            ng_ios_pen_sample(view, NG_IOS_PEN_PREDICTED, ahead);
        }
    }
}

void ng_ios_pen_hover(UIView* view, UIHoverGestureRecognizer* hover) {
    if (!g_ios_host.pen) return;
    if (@available(iOS 16.4, *)) {
        if (hover.state != UIGestureRecognizerStateBegan && hover.state != UIGestureRecognizerStateChanged) return;
        CGPoint at = [hover locationInView:view];
        float tilt_x = 0, tilt_y = 0, twist = 0;
        ng_ios_tilt(hover.altitudeAngle, [hover azimuthAngleInView:view], &tilt_x, &tilt_y);
        if (@available(iOS 17.5, *)) twist = (float)(hover.rollAngle * 180.0 / M_PI);
        g_ios_host.pen(NG_IOS_PEN_HOVER, at.x, at.y, 0.0f, tilt_x, tilt_y, twist, 0.0);
        [g_ios_view markDirty];
    }
}

/* What the user set a pencil gesture to do, as PenTapAction codes: palettes of any kind are one,
   and system shortcuts another. Compared as numbers, since the later actions are newer than the
   oldest iOS this builds for. */
static int ng_ios_pen_action(long action) {
    switch (action) {
        case 0: return 0;
        case 1: return 1;
        case 2: return 2;
        case 3: case 4: case 5: return 3;
        default: return 4;
    }
}

static void ng_ios_pen_tap(int squeeze, long action) {
    if (g_ios_host.pen_tap) g_ios_host.pen_tap(squeeze, ng_ios_pen_action(action));
    [g_ios_view markDirty];
}

API_AVAILABLE(ios(12.1))
@interface AureaPencilDelegate : NSObject <UIPencilInteractionDelegate>
@end

@implementation AureaPencilDelegate

/* A double tap on an Apple Pencil's flat side. */
- (void)pencilInteraction:(UIPencilInteraction*)interaction
            didReceiveTap:(UIPencilInteractionTap*)tap API_AVAILABLE(ios(17.5)) {
    (void)interaction;
    (void)tap;
    ng_ios_pen_tap(0, (long)UIPencilInteraction.preferredTapAction);
}

/* A squeeze of a Pencil Pro, once it lets go. */
- (void)pencilInteraction:(UIPencilInteraction*)interaction
        didReceiveSqueeze:(UIPencilInteractionSqueeze*)squeeze API_AVAILABLE(ios(17.5)) {
    (void)interaction;
    if (squeeze.phase != UIPencilInteractionPhaseEnded) return;
    ng_ios_pen_tap(1, (long)UIPencilInteraction.preferredSqueezeAction);
}

/* Before iOS 17.5 a tap comes here; later ones come above instead. */
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#pragma clang diagnostic ignored "-Wdeprecated-implementations"
- (void)pencilInteractionDidTap:(UIPencilInteraction*)interaction {
    (void)interaction;
    ng_ios_pen_tap(0, (long)UIPencilInteraction.preferredTapAction);
}
#pragma clang diagnostic pop

@end

void ng_ios_pen_attach(UIView* view) {
    if (@available(iOS 12.1, *)) {
        /* the interaction keeps its delegate weak, the one view keeps this one */
        static AureaPencilDelegate* delegate;
        if (!delegate) delegate = [[AureaPencilDelegate alloc] init];
        UIPencilInteraction* pencil = [[UIPencilInteraction alloc] init];
        pencil.delegate = delegate;
        [view addInteraction:pencil];
    }
}
