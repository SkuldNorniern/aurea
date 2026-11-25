#import "text.h"
#import "ime.h"
#import "content_view.h"
#import "common/rust_callbacks.h"

/* The view as a text client: what the input method makes goes to the app as TextInput, what it
   is composing as ImePreedit. */
@interface AureaContentView (Text) <NSTextInputClient>
- (void)commitMarkedText:(NSEvent*)event;
- (void)dropMarkedText;
- (BOOL)interpretKey:(NSEvent*)event;
@end

@implementation AureaContentView (Text)

/* A press commits what an input method is composing, before the press goes out.
   input method sees the press first like in a text view, it commits by itself.
   if it keeps the text, put ours in and make it drop its copy so it cant come twice. */
- (void)commitMarkedText:(NSEvent*)event {
    if (![self hasMarkedText]) return;
    [[self inputContext] handleEvent:event];
    if (![self hasMarkedText]) return;
    NSString* text = markedText;
    [self insertText:text replacementRange:NSMakeRange(NSNotFound, 0)];
    [[self inputContext] discardMarkedText];
}

/* focus loss cancels the composition on the Rust side, drop ours too
   or keyDown keeps holding keys back after focus comes back */
- (void)dropMarkedText {
    if (![self hasMarkedText]) return;
    markedText = nil;
    [[self inputContext] discardMarkedText];
}

/* no context while the input method is kept out, so nothing composes */
- (NSTextInputContext*)inputContext {
    return self.imeBlocked ? nil : [super inputContext];
}

/* interpretKeyEvents ends in insertText, setMarkedText or doCommandBySelector. Without a
   context it types nothing, so a kept-out input method types the key's own characters. */
- (BOOL)interpretKey:(NSEvent*)event {
    imeSentCommand = NO;
    if (self.imeBlocked) {
        NSEventModifierFlags held =
            event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl);
        NSString* chars = event.characters;
        if (!held && chars.length > 0) {
            [self insertText:chars replacementRange:NSMakeRange(NSNotFound, 0)];
        }
        return NO;
    }
    [self interpretKeyEvents:@[event]];
    return imeSentCommand;
}

- (void)insertText:(id)string replacementRange:(NSRange)replacementRange {
    (void)replacementRange;
    NSString* text = [string isKindOfClass:[NSAttributedString class]] ? [string string] : string;
    if ([self hasMarkedText]) {
        markedText = nil;
        ng_macos_ime_preedit(self, nil, NSMakeRange(NSNotFound, 0));
    }
    if (!self.windowHandle || !text || [text length] == 0) return;

    unichar first = [text characterAtIndex:0];
    BOOL isControl = (first < 0x0020 || first == 0x007F);
    BOOL isFnKeyChar = (first >= 0xF700 && first <= 0xF8FF);
    if (isControl || isFnKeyChar) return;

    const char* utf8 = [text UTF8String];
    if (utf8) {
        ng_invoke_text_input(self.windowHandle, utf8);
    }
}

- (void)setMarkedText:(id)string selectedRange:(NSRange)selectedRange replacementRange:(NSRange)replacementRange {
    (void)replacementRange;
    NSString* text = [string isKindOfClass:[NSAttributedString class]] ? [string string] : string;
    markedText = (text && [text length] > 0) ? [text copy] : nil;
    ng_macos_ime_preedit(self, markedText, selectedRange);
}

- (void)unmarkText {
    if (![self hasMarkedText]) return;
    markedText = nil;
    ng_macos_ime_preedit(self, nil, NSMakeRange(NSNotFound, 0));
}

- (BOOL)hasMarkedText {
    return markedText != nil && [markedText length] > 0;
}

- (NSRange)markedRange {
    return [self hasMarkedText] ? NSMakeRange(0, [markedText length]) : NSMakeRange(NSNotFound, 0);
}

- (NSRange)selectedRange {
    return NSMakeRange(NSNotFound, 0);
}

- (NSArray<NSAttributedStringKey>*)validAttributesForMarkedText {
    return @[];
}

- (NSAttributedString*)attributedSubstringForProposedRange:(NSRange)range actualRange:(NSRangePointer)actualRange {
    (void)range;
    (void)actualRange;
    return nil;
}

- (NSUInteger)characterIndexForPoint:(NSPoint)point {
    (void)point;
    return NSNotFound;
}

- (NSRect)firstRectForCharacterRange:(NSRange)range actualRange:(NSRangePointer)actualRange {
    (void)range;
    (void)actualRange;
    CGFloat h = [self bounds].size.height;
    NSRect local = NSMakeRect(
        self.imeArea.origin.x,
        h - self.imeArea.origin.y - self.imeArea.size.height,
        self.imeArea.size.width,
        self.imeArea.size.height);
    NSRect inWindow = [self convertRect:local toView:nil];
    return [self window] ? [[self window] convertRectToScreen:inWindow] : inWindow;
}

- (void)doCommandBySelector:(SEL)selector {
    (void)selector;
    imeSentCommand = YES;
}

@end

BOOL ng_macos_text_composing(AureaContentView* view) {
    return [view hasMarkedText];
}

BOOL ng_macos_text_key(AureaContentView* view, NSEvent* event) {
    return [view interpretKey:event];
}

void ng_macos_text_commit(AureaContentView* view, NSEvent* event) {
    [view commitMarkedText:event];
}

void ng_macos_text_drop(AureaContentView* view) {
    [view dropMarkedText];
}
