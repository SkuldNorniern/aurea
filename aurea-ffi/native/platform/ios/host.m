#import "host.h"
#import "../../common/input.h"
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#include <crt_externs.h>
#include <math.h>
#include <string.h>

/* One Aurea UI filling the screen: the app draws each frame of its safe area into a pixel buffer,
   which becomes a sublayer's contents; the view's own colour fills the rest. A display link asks for frames while the UI wants them, and pauses when
   it has had nothing to do for a while. */

static NGIosHost g_host;

/* How many idle refreshes before the display link pauses: a quarter second at 120 Hz. */
#define NG_IOS_IDLE_TICKS 30

/* The side of a presented tile in pixels: a multiple of the rasterizer's 128 pixel tiles. */
#define NG_IOS_TILE 256u

@class AureaKeyInput;

@interface AureaHostView : UIView <UIPencilInteractionDelegate>
@property (strong) CADisplayLink* link;
@property (assign) BOOL dirty;
@property (assign) int idle;
@property (strong) CALayer* content;
@property (assign) uint32_t background;
@property (strong) NSMutableArray<CALayer*>* tiles;
@property (strong) UIView* gpuView;
@property (assign) unsigned gridWidth;
@property (assign) unsigned gridHeight;
@property (assign) CGFloat gridScale;
@property (strong) AureaKeyInput* keys;
@property (assign) CGFloat keyboardHeight;
- (void)wakeLink;
- (void)markDirty;
- (NSSet<UIPress*>*)keys:(NSSet<UIPress*>*)presses pressed:(int)pressed;
@end

static __weak AureaHostView* g_view;

/* A place in the text being composed, as a UTF-16 index into it. */
@interface AureaTextPosition : UITextPosition
@property (assign) NSInteger index;
+ (instancetype)at:(NSInteger)index;
@end

@implementation AureaTextPosition
+ (instancetype)at:(NSInteger)index {
    AureaTextPosition* position = [[AureaTextPosition alloc] init];
    position.index = index;
    return position;
}
@end

@interface AureaTextRange : UITextRange
@property (strong) AureaTextPosition* from;
@property (strong) AureaTextPosition* to;
+ (instancetype)from:(NSInteger)from to:(NSInteger)to;
@end

@implementation AureaTextRange
+ (instancetype)from:(NSInteger)from to:(NSInteger)to {
    AureaTextRange* range = [[AureaTextRange alloc] init];
    range.from = [AureaTextPosition at:MIN(from, to)];
    range.to = [AureaTextPosition at:MAX(from, to)];
    return range;
}
- (UITextPosition*)start {
    return _from;
}
- (UITextPosition*)end {
    return _to;
}
- (BOOL)isEmpty {
    return _from.index == _to.index;
}
@end

/* Takes text from the on-screen keyboard while it is up; see ng_ios_show_keyboard. Its document is
   only the text an input method is composing, sent as `preedit`; what is typed or composed goes to
   `key` as text with no key code, delete and return as Backspace and Enter. A hardware keyboard's
   presses go where the host view's do, so text does not come twice. */
@interface AureaKeyInput : UIView <UITextInput>
@property (copy) NSString* marked;
@property (assign) NSInteger caret;
@property (assign) CGRect area;
@end

@implementation AureaKeyInput

@synthesize inputDelegate;
@synthesize markedTextStyle;

- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self) _marked = @"";
    return self;
}

- (BOOL)canBecomeFirstResponder {
    return YES;
}

- (UITextAutocorrectionType)autocorrectionType {
    return UITextAutocorrectionTypeNo;
}

- (UITextAutocapitalizationType)autocapitalizationType {
    return UITextAutocapitalizationTypeNone;
}

- (UITextSpellCheckingType)spellCheckingType {
    return UITextSpellCheckingTypeNo;
}

static void ng_ios_tap_key(uint32_t code) {
    if (!g_host.key) return;
    g_host.key(code, 1, 0, "");
    g_host.key(code, 0, 0, "");
}

static void ng_ios_type(NSString* text) {
    if (g_host.key && text.length) g_host.key(NG_KEY_UNKNOWN, 1, 0, text.UTF8String ?: "");
}

/* The composition as the app sees it, the cursor in UTF-8 bytes. */
- (void)sendPreedit {
    if (g_host.preedit) {
        NSInteger caret = MIN(MAX(_caret, 0), (NSInteger)_marked.length);
        NSString* head = [_marked substringToIndex:(NSUInteger)caret];
        int cursor = _marked.length ? (int)[head lengthOfBytesUsingEncoding:NSUTF8StringEncoding] : -1;
        g_host.preedit(_marked.UTF8String ?: "", cursor);
    }
    [g_view markDirty];
}

- (void)endComposing {
    if (!_marked.length) return;
    _marked = @"";
    _caret = 0;
    [self sendPreedit];
}

/* UIKeyInput. */

- (BOOL)hasText {
    return YES;
}

- (void)insertText:(NSString*)text {
    [self endComposing];
    if ([text isEqualToString:@"\n"]) {
        ng_ios_tap_key(NG_KEY_ENTER);
    } else {
        ng_ios_type(text);
    }
    [g_view markDirty];
}

- (void)deleteBackward {
    if (_marked.length) {
        NSRange last = [_marked rangeOfComposedCharacterSequenceAtIndex:_marked.length - 1];
        _marked = [_marked substringToIndex:last.location];
        _caret = (NSInteger)_marked.length;
        [self sendPreedit];
        return;
    }
    ng_ios_tap_key(NG_KEY_BACKSPACE);
    [g_view markDirty];
}

/* Composition. */

- (UITextRange*)markedTextRange {
    return _marked.length ? [AureaTextRange from:0 to:(NSInteger)_marked.length] : nil;
}

- (void)setMarkedText:(NSString*)text selectedRange:(NSRange)selected {
    _marked = [text copy] ?: @"";
    _caret = selected.location == NSNotFound ? (NSInteger)_marked.length : (NSInteger)selected.location;
    [self sendPreedit];
}

/* The input method is done with the composition: it is typed as it stands. */
- (void)unmarkText {
    NSString* done = _marked;
    [self endComposing];
    ng_ios_type(done);
}

/* The document: the composition alone. */

- (NSString*)textInRange:(UITextRange*)range {
    AureaTextRange* r = (AureaTextRange*)range;
    NSInteger from = MIN(MAX(r.from.index, 0), (NSInteger)_marked.length);
    NSInteger to = MIN(MAX(r.to.index, from), (NSInteger)_marked.length);
    return [_marked substringWithRange:NSMakeRange((NSUInteger)from, (NSUInteger)(to - from))];
}

- (void)replaceRange:(UITextRange*)range withText:(NSString*)text {
    (void)range;
    [self insertText:text];
}

- (UITextRange*)selectedTextRange {
    return [AureaTextRange from:_caret to:_caret];
}

- (void)setSelectedTextRange:(UITextRange*)range {
    _caret = ((AureaTextRange*)range).from.index;
}

- (UITextPosition*)beginningOfDocument {
    return [AureaTextPosition at:0];
}

- (UITextPosition*)endOfDocument {
    return [AureaTextPosition at:(NSInteger)_marked.length];
}

- (UITextRange*)textRangeFromPosition:(UITextPosition*)from toPosition:(UITextPosition*)to {
    return [AureaTextRange from:((AureaTextPosition*)from).index to:((AureaTextPosition*)to).index];
}

- (UITextPosition*)positionFromPosition:(UITextPosition*)position offset:(NSInteger)offset {
    NSInteger at = ((AureaTextPosition*)position).index + offset;
    if (at < 0 || at > (NSInteger)_marked.length) return nil;
    return [AureaTextPosition at:at];
}

- (UITextPosition*)positionFromPosition:(UITextPosition*)position
                            inDirection:(UITextLayoutDirection)direction
                                 offset:(NSInteger)offset {
    BOOL back = direction == UITextLayoutDirectionLeft || direction == UITextLayoutDirectionUp;
    return [self positionFromPosition:position offset:back ? -offset : offset];
}

- (NSComparisonResult)comparePosition:(UITextPosition*)position toPosition:(UITextPosition*)other {
    NSInteger a = ((AureaTextPosition*)position).index, b = ((AureaTextPosition*)other).index;
    return a < b ? NSOrderedAscending : a > b ? NSOrderedDescending : NSOrderedSame;
}

- (NSInteger)offsetFromPosition:(UITextPosition*)from toPosition:(UITextPosition*)to {
    return ((AureaTextPosition*)to).index - ((AureaTextPosition*)from).index;
}

- (id<UITextInputTokenizer>)tokenizer {
    return [[UITextInputStringTokenizer alloc] initWithTextInput:self];
}

- (UITextPosition*)positionWithinRange:(UITextRange*)range farthestInDirection:(UITextLayoutDirection)direction {
    BOOL back = direction == UITextLayoutDirectionLeft || direction == UITextLayoutDirectionUp;
    return back ? range.start : range.end;
}

- (UITextRange*)characterRangeByExtendingPosition:(UITextPosition*)position
                                      inDirection:(UITextLayoutDirection)direction {
    NSInteger at = ((AureaTextPosition*)position).index;
    BOOL back = direction == UITextLayoutDirectionLeft || direction == UITextLayoutDirectionUp;
    return [AureaTextRange from:at to:back ? MAX(at - 1, 0) : MIN(at + 1, (NSInteger)_marked.length)];
}

- (NSWritingDirection)baseWritingDirectionForPosition:(UITextPosition*)position
                                          inDirection:(UITextStorageDirection)direction {
    (void)position;
    (void)direction;
    return NSWritingDirectionNatural;
}

- (void)setBaseWritingDirection:(NSWritingDirection)direction forRange:(UITextRange*)range {
    (void)direction;
    (void)range;
}

/* Geometry: the app says where text is being typed with ng_ios_set_text_area, so candidate lists
   show beside it. */

- (CGRect)firstRectForRange:(UITextRange*)range {
    (void)range;
    return _area;
}

- (CGRect)caretRectForPosition:(UITextPosition*)position {
    (void)position;
    return CGRectMake(CGRectGetMaxX(_area), _area.origin.y, 2, _area.size.height);
}

- (NSArray<UITextSelectionRect*>*)selectionRectsForRange:(UITextRange*)range {
    (void)range;
    return @[];
}

- (UITextPosition*)closestPositionToPoint:(CGPoint)point {
    (void)point;
    return [self endOfDocument];
}

- (UITextPosition*)closestPositionToPoint:(CGPoint)point withinRange:(UITextRange*)range {
    (void)point;
    return range.end;
}

- (UITextRange*)characterRangeAtPoint:(CGPoint)point {
    (void)point;
    return nil;
}

/* Hardware keys go to the host's handler. */

- (void)pressesBegan:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [g_view keys:presses pressed:1];
    if (rest.count) [super pressesBegan:rest withEvent:event];
}

- (void)pressesEnded:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [g_view keys:presses pressed:0];
    if (rest.count) [super pressesEnded:rest withEvent:event];
}

- (void)pressesCancelled:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [g_view keys:presses pressed:0];
    if (rest.count) [super pressesCancelled:rest withEvent:event];
}

@end

@implementation AureaHostView

- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.multipleTouchEnabled = YES;
        _content = [CALayer layer];
        _tiles = [NSMutableArray array];
        [self.layer addSublayer:_content];
        _background = UINT32_MAX;
        _dirty = YES;
        g_view = self;
        if (@available(iOS 12.1, *)) {
            UIPencilInteraction* pencil = [[UIPencilInteraction alloc] init];
            pencil.delegate = self;
            [self addInteraction:pencil];
        }
        _keys = [[AureaKeyInput alloc] initWithFrame:CGRectMake(0, 0, 1, 1)];
        _keys.hidden = YES;
        [self addSubview:_keys];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(keyboardChanged:)
                                                     name:UIKeyboardWillChangeFrameNotification
                                                   object:nil];
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

/* How much of the view the on-screen keyboard covers, for ng_ios_keyboard_height. */
- (void)keyboardChanged:(NSNotification*)note {
    CGRect end = [note.userInfo[UIKeyboardFrameEndUserInfoKey] CGRectValue];
    CGRect mine = [self convertRect:end fromCoordinateSpace:self.window.screen.coordinateSpace];
    _keyboardHeight = MAX(0, CGRectGetMaxY(self.bounds) - CGRectGetMinY(mine));
    [self markDirty];
}

- (void)memoryWarning:(NSNotification*)note {
    (void)note;
    if (g_host.lifecycle) g_host.lifecycle(NG_IOS_MEMORY_WARNING);
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
    if (!g_host.background) return;
    uint32_t rgb = g_host.background() & 0xFFFFFF;
    if (rgb == _background) return;
    _background = rgb;
    self.backgroundColor = [UIColor colorWithRed:((rgb >> 16) & 0xFF) / 255.0
                                           green:((rgb >> 8) & 0xFF) / 255.0
                                            blue:(rgb & 0xFF) / 255.0
                                           alpha:1.0];
}

- (void)tick:(CADisplayLink*)link {
    if (!g_host.frame) return;
    /* `wake` is how other threads, and touches still being told apart, ask for a frame. */
    if (!_dirty && !(g_host.wake && g_host.wake())) {
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
    _dirty = g_host.frame(width, height, (float)scale, (float)safe.left, (float)safe.top, (float)safe.right,
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
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
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
    CGColorSpaceRelease(space);
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
    /* Where it will likely go next, while it is down, for the stroke to seem to keep up. */
    if (phase == NG_IOS_PEN_DOWN || phase == NG_IOS_PEN_MOVE) {
        for (UITouch* ahead in [event predictedTouchesForTouch:touch]) {
            [self pen:NG_IOS_PEN_PREDICTED touch:ahead];
        }
    }
}

- (void)send:(int)action touches:(NSSet<UITouch*>*)touches event:(UIEvent*)event {
    static const int pen_phase[] = {NG_IOS_PEN_DOWN, NG_IOS_PEN_UP, NG_IOS_PEN_MOVE, NG_IOS_PEN_UP};
    for (UITouch* touch in touches) {
        [self pens:pen_phase[action] touch:touch event:event];
        if (g_host.touch) {
            CGPoint at = [touch locationInView:self];
            int pencil = touch.type == UITouchTypePencil;
            g_host.touch(action, (float)at.x, (float)at.y, (uintptr_t)(__bridge void*)touch, pencil);
        }
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
    if (!g_host.key) return presses;
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
            g_host.key(ng_ios_key(key.keyCode), pressed, mods, typed.UTF8String ?: "");
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

- (void)penTap:(int)squeeze action:(long)action {
    if (g_host.pen_tap) g_host.pen_tap(squeeze, ng_ios_pen_action(action));
    [self markDirty];
}

/* A double tap on an Apple Pencil's flat side. */
- (void)pencilInteraction:(UIPencilInteraction*)interaction
            didReceiveTap:(UIPencilInteractionTap*)tap API_AVAILABLE(ios(17.5)) {
    (void)interaction;
    (void)tap;
    [self penTap:0 action:(long)UIPencilInteraction.preferredTapAction];
}

/* A squeeze of a Pencil Pro, once it lets go. */
- (void)pencilInteraction:(UIPencilInteraction*)interaction
        didReceiveSqueeze:(UIPencilInteractionSqueeze*)squeeze API_AVAILABLE(ios(17.5)) {
    (void)interaction;
    if (squeeze.phase != UIPencilInteractionPhaseEnded) return;
    [self penTap:1 action:(long)UIPencilInteraction.preferredSqueezeAction];
}

/* Before iOS 17.5 a tap comes here; later ones come above instead. */
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#pragma clang diagnostic ignored "-Wdeprecated-implementations"
- (void)pencilInteractionDidTap:(UIPencilInteraction*)interaction API_AVAILABLE(ios(12.1)) {
    (void)interaction;
    [self penTap:0 action:(long)UIPencilInteraction.preferredTapAction];
}
#pragma clang diagnostic pop

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
        [self markDirty];
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

static void ng_ios_lifecycle(int state) {
    if (g_host.lifecycle) g_host.lifecycle(state);
    [g_view markDirty];
}

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
    ng_ios_lifecycle(NG_IOS_BACKGROUND);
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
    g_host = host;
    @autoreleasepool {
        return UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), nil, NSStringFromClass([AureaHostDelegate class]));
    }
}

void ng_ios_wake(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [g_view wakeLink];
    });
}

void* ng_ios_gpu_view(void) {
    AureaHostView* host = g_view;
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
    AureaHostView* host = g_view;
    [host.gpuView removeFromSuperview];
    host.gpuView = nil;
}

void ng_ios_show_keyboard(int show) {
    AureaHostView* host = g_view;
    if (!host) return;
    if (show) {
        [host.keys becomeFirstResponder];
    } else if (host.keys.isFirstResponder) {
        [host becomeFirstResponder];
    }
}

float ng_ios_refresh_rate(void) {
    AureaHostView* host = g_view;
    UIScreen* screen = host.window.screen ?: UIScreen.mainScreen;
    return (float)screen.maximumFramesPerSecond;
}

void ng_ios_set_high_frame_rate(int high) {
    AureaHostView* host = g_view;
    if (!host) return;
    if (@available(iOS 15.0, *)) {
        float top = ng_ios_refresh_rate();
        host.link.preferredFrameRateRange =
            high && top > 60 ? CAFrameRateRangeMake(80, top, top) : CAFrameRateRangeDefault;
    }
}

float ng_ios_keyboard_height(void) {
    AureaHostView* host = g_view;
    return host && host.keys.isFirstResponder ? (float)host.keyboardHeight : 0.0f;
}

void ng_ios_set_text_area(float x, float y, float width, float height) {
    AureaHostView* host = g_view;
    if (!host) return;
    UIEdgeInsets safe = host.safeAreaInsets;
    host.keys.area = CGRectMake(x + safe.left, y + safe.top, width, height);
}
