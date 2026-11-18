#import "text.h"
#import "ime.h"
#import "host_view.h"
#import "../../common/input.h"

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
/* a password: the keyboard composes and predicts nothing */
@property (assign) BOOL secure;
- (void)commitComposing;
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

- (BOOL)isSecureTextEntry {
    return self.secure;
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
    if (!g_ios_host.key) return;
    g_ios_host.key(code, 1, 0, "");
    g_ios_host.key(code, 0, 0, "");
}

static void ng_ios_type(NSString* text) {
    if (g_ios_host.key && text.length) g_ios_host.key(NG_KEY_UNKNOWN, 1, 0, text.UTF8String ?: "");
}

/* The composition to the app. */
- (void)sendPreedit {
    ng_ios_ime_preedit(_marked, _caret);
    [g_ios_view markDirty];
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
    [g_ios_view markDirty];
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
    [g_ios_view markDirty];
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

/* a press commits the composition like a text view, keyboard is told so it starts fresh
   and the next jamo does not join the old syllable */
- (void)commitComposing {
    if (!_marked.length) return;
    [self.inputDelegate textWillChange:self];
    [self unmarkText];
    [self.inputDelegate textDidChange:self];
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
    NSSet<UIPress*>* rest = [g_ios_view keys:presses pressed:1];
    if (rest.count) [super pressesBegan:rest withEvent:event];
}

- (void)pressesEnded:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [g_ios_view keys:presses pressed:0];
    if (rest.count) [super pressesEnded:rest withEvent:event];
}

- (void)pressesCancelled:(NSSet<UIPress*>*)presses withEvent:(UIPressesEvent*)event {
    NSSet<UIPress*>* rest = [g_ios_view keys:presses pressed:0];
    if (rest.count) [super pressesCancelled:rest withEvent:event];
}

@end

void ng_ios_text_attach(AureaHostView* host) {
    AureaKeyInput* keys = [[AureaKeyInput alloc] initWithFrame:CGRectMake(0, 0, 1, 1)];
    keys.hidden = YES;
    [host addSubview:keys];
    host.keys = keys;
}

void ng_ios_set_secure_text(int secure) {
    AureaKeyInput* keys = g_ios_view.keys;
    if (!keys || keys.secure == (secure != 0)) return;
    [keys commitComposing];
    keys.secure = secure != 0;
    /* a keyboard already up takes the new traits only when reloaded */
    if (keys.isFirstResponder) [keys reloadInputViews];
}

void ng_ios_text_commit(AureaHostView* host) {
    [host.keys commitComposing];
}

void ng_ios_show_keyboard(int show) {
    AureaHostView* host = g_ios_view;
    if (!host) return;
    if (show) {
        [host.keys becomeFirstResponder];
    } else if (host.keys.isFirstResponder) {
        [host becomeFirstResponder];
    }
}

float ng_ios_keyboard_height(void) {
    AureaHostView* host = g_ios_view;
    return host && host.keys.isFirstResponder ? (float)host.keyboardHeight : 0.0f;
}

void ng_ios_set_text_area(float x, float y, float width, float height) {
    AureaHostView* host = g_ios_view;
    if (!host) return;
    UIEdgeInsets safe = host.safeAreaInsets;
    host.keys.area = CGRectMake(x + safe.left, y + safe.top, width, height);
}
