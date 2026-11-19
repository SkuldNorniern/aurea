#import <UIKit/UIKit.h>

#import "accessibility.h"
#import "host_view.h"

/* The drawn UI to VoiceOver: one element per control the UI described, kept by id so the cursor
   stays on its control from frame to frame. Native views shown over it follow in their order. */

static NGAxAction g_ax_action;
static NGAxActivate g_ax_activate;
static BOOL g_ax_active;
static NSMutableArray* g_ax_elements;
static NSMutableDictionary<NSNumber*, UIAccessibilityElement*>* g_ax_by_id;
static uint64_t g_ax_focus;

@interface AureaAXElement : UIAccessibilityElement
@property (assign) uint64_t axId;
@end

@implementation AureaAXElement

- (BOOL)accessibilityActivate {
    if (!g_ax_action) return NO;
    g_ax_action(self.axId, NG_AX_ACTIVATE);
    return YES;
}

- (void)accessibilityIncrement {
    if (g_ax_action) g_ax_action(self.axId, NG_AX_INCREMENT);
}

- (void)accessibilityDecrement {
    if (g_ax_action) g_ax_action(self.axId, NG_AX_DECREMENT);
}

@end

static void ng_ios_ax_activate(void) {
    if (g_ax_active || !g_ax_activate) return;
    g_ax_active = YES;
    g_ax_activate();
}

@implementation AureaHostView (Accessibility)

- (BOOL)isAccessibilityElement {
    return NO;
}

- (NSArray*)accessibilityElements {
    ng_ios_ax_activate();
    NSMutableArray* all = [NSMutableArray arrayWithArray:g_ax_elements ?: @[]];
    for (UIView* view in self.subviews) {
        if (view.hidden || view == (UIView*)self.keys || view == self.gpuView) continue;
        [all addObject:view];
    }
    return all;
}

@end

static UIAccessibilityTraits ng_ios_ax_traits(uint32_t t) {
    UIAccessibilityTraits out = UIAccessibilityTraitNone;
    if (t & NG_AX_BUTTON) out |= UIAccessibilityTraitButton;
    if (t & NG_AX_LINK) out |= UIAccessibilityTraitLink;
    if (t & NG_AX_HEADER) out |= UIAccessibilityTraitHeader;
    if (t & NG_AX_STATIC_TEXT) out |= UIAccessibilityTraitStaticText;
    if (t & NG_AX_ADJUSTABLE) out |= UIAccessibilityTraitAdjustable;
    if (t & NG_AX_SELECTED) out |= UIAccessibilityTraitSelected;
    if (t & NG_AX_NOT_ENABLED) out |= UIAccessibilityTraitNotEnabled;
    if (t & NG_AX_IMAGE) out |= UIAccessibilityTraitImage;
    /* UIKit has no public trait for a text field; its value and label still read */
    return out;
}

static NSString* ng_ios_ax_text(const char* text) {
    if (!text || !*text) return nil;
    return [NSString stringWithUTF8String:text];
}

void ng_ios_ax_set_handler(NGAxAction action, NGAxActivate activate) {
    g_ax_action = action;
    g_ax_activate = activate;
    g_ax_active = NO;
    if (!activate) return;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        [[NSNotificationCenter defaultCenter]
            addObserverForName:UIAccessibilityVoiceOverStatusDidChangeNotification
                        object:nil
                         queue:[NSOperationQueue mainQueue]
                    usingBlock:^(NSNotification* note) {
                        (void)note;
                        if (UIAccessibilityIsVoiceOverRunning()) ng_ios_ax_activate();
                    }];
    });
    if (UIAccessibilityIsVoiceOverRunning() || UIAccessibilityIsSwitchControlRunning()) {
        ng_ios_ax_activate();
    }
}

void ng_ios_ax_update(const NGAxElement* elements, size_t count, uint64_t focus) {
    AureaHostView* host = g_ios_view;
    if (!host) return;
    if (!g_ax_by_id) g_ax_by_id = [NSMutableDictionary dictionary];
    UIEdgeInsets safe = host.safeAreaInsets;
    NSMutableArray* next = [NSMutableArray arrayWithCapacity:count];
    NSMutableDictionary* kept = [NSMutableDictionary dictionaryWithCapacity:count];
    for (size_t i = 0; i < count; i++) {
        const NGAxElement* e = &elements[i];
        NSNumber* key = @(e->id);
        AureaAXElement* el = (AureaAXElement*)g_ax_by_id[key];
        if (!el) {
            el = [[AureaAXElement alloc] initWithAccessibilityContainer:host];
            el.axId = e->id;
        }
        el.accessibilityLabel = ng_ios_ax_text(e->label);
        el.accessibilityValue = ng_ios_ax_text(e->value);
        el.accessibilityHint = ng_ios_ax_text(e->hint);
        el.accessibilityTraits = ng_ios_ax_traits(e->traits);
        el.accessibilityFrameInContainerSpace =
            CGRectMake(e->x + safe.left, e->y + safe.top, e->width, e->height);
        [next addObject:el];
        kept[key] = el;
    }
    BOOL moved = ![next isEqualToArray:g_ax_elements ?: @[]];
    g_ax_elements = next;
    g_ax_by_id = kept;
    UIAccessibilityElement* focused = focus != g_ax_focus ? kept[@(focus)] : nil;
    g_ax_focus = focus;
    if (focused) {
        /* the app moved its focus (a field took it): the cursor follows */
        UIAccessibilityPostNotification(UIAccessibilityLayoutChangedNotification, focused);
    } else if (moved) {
        UIAccessibilityPostNotification(UIAccessibilityLayoutChangedNotification, nil);
    }
}
