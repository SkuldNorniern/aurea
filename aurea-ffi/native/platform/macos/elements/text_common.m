#import "../elements.h"
#import "../utils.h"
#import "common/errors.h"
#import "common/platform_api.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>
#import <string.h>

static char kQuietKey;
static char kUndoKey;
static char kWatchKey;

@interface NGTextDelegate : NSObject <NSTextViewDelegate>
@end

@implementation NGTextDelegate
- (NSUndoManager*)undoManagerForTextView:(NSTextView*)view {
    NSUndoManager* undo = objc_getAssociatedObject(view, &kUndoKey);
    if (!undo) {
        undo = [[NSUndoManager alloc] init];
        objc_setAssociatedObject(view, &kUndoKey, undo, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
    return undo;
}
@end

@interface NGTextWatch : NSObject
@property (strong) id token;
@end

@implementation NGTextWatch
- (void)dealloc {
    if (_token) [[NSNotificationCenter defaultCenter] removeObserver:_token];
}
@end

static NSTextView* text_view_of(NGHandle handle) {
    NSView* view = (__bridge NSView*)handle;
    if (![view isKindOfClass:[NSScrollView class]]) return nil;
    NSView* doc = [(NSScrollView*)view documentView];
    return [doc isKindOfClass:[NSTextView class]] ? (NSTextView*)doc : nil;
}

static NSTextField* field_of(NGHandle handle) {
    NSView* view = (__bridge NSView*)handle;
    return [view isKindOfClass:[NSTextField class]] ? (NSTextField*)view : nil;
}

static void set_quiet(id object, BOOL quiet) {
    objc_setAssociatedObject(object, &kQuietKey, quiet ? @YES : nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

/* AppKit only lets a secure cell edit inside an NSSecureTextField, so every
   field is one, with a plain cell until it is made secret. */
@interface NGTextField : NSSecureTextField
@end

@implementation NGTextField
+ (Class)cellClass {
    return [NSTextFieldCell class];
}

- (BOOL)isSecret {
    return [self.cell isKindOfClass:[NSSecureTextFieldCell class]];
}

- (NSAccessibilitySubrole)accessibilitySubrole {
    return [self isSecret] ? NSAccessibilitySecureTextFieldSubrole : nil;
}

- (id)accessibilityValue {
    return [self isSecret] ? [super accessibilityValue] : self.stringValue;
}
@end

NGHandle ng_macos_create_text_field(void) {
    @autoreleasepool {
        NSTextField* field = [[NGTextField alloc] init];
        [field setEditable:YES];
        [field setSelectable:YES];
        [field setBordered:YES];
        [field setBezeled:YES];
        [field setBezelStyle:NSTextFieldRoundedBezel];
        [field setFont:[NSFont systemFontOfSize:13.0]];
        [field setTranslatesAutoresizingMaskIntoConstraints:NO];
        [[field.heightAnchor constraintGreaterThanOrEqualToConstant:24.0] setActive:YES];
        return (__bridge_retained void*)field;
    }
}

NGHandle ng_macos_create_text_area(int editable, double min_height) {
    @autoreleasepool {
        NSScrollView* scrollView = [[NSScrollView alloc] init];
        NSTextView* textView = [[NSTextView alloc] init];

        [scrollView setHasVerticalScroller:YES];
        [scrollView setHasHorizontalScroller:YES];
        [scrollView setAutohidesScrollers:YES];
        [scrollView setBorderType:NSBezelBorder];
        [scrollView setTranslatesAutoresizingMaskIntoConstraints:NO];

        [textView setEditable:editable ? YES : NO];
        [textView setSelectable:YES];
        [textView setRichText:NO];
        [textView setAllowsUndo:YES];
        [textView setAutomaticQuoteSubstitutionEnabled:NO];
        [textView setAutomaticDashSubstitutionEnabled:NO];
        [textView setEnabledTextCheckingTypes:0];
        [textView setFont:[NSFont systemFontOfSize:[NSFont systemFontSize]]];
        [textView setTextContainerInset:NSMakeSize(5, 5)];
        [[textView textContainer] setWidthTracksTextView:YES];
        [[textView textContainer] setHeightTracksTextView:NO];
        [textView setHorizontallyResizable:NO];
        [textView setVerticallyResizable:YES];
        [textView setMaxSize:NSMakeSize(FLT_MAX, FLT_MAX)];
        [textView setAutoresizingMask:NSViewWidthSizable];

        NGTextDelegate* delegate = [[NGTextDelegate alloc] init];
        [textView setDelegate:delegate];
        objc_setAssociatedObject(textView, "delegate", delegate, OBJC_ASSOCIATION_RETAIN_NONATOMIC);

        [scrollView setDocumentView:textView];
        [[scrollView.heightAnchor constraintGreaterThanOrEqualToConstant:min_height] setActive:YES];
        return (__bridge_retained void*)scrollView;
    }
}

int ng_macos_text_watch_change(NGHandle element, unsigned int change_id) {
    NSTextView* view = text_view_of(element);
    NSTextField* field = field_of(element);
    if (!view && !field) return 0;
    id source = view ? (id)view : (id)field;
    NSNotificationName name = view ? NSTextDidChangeNotification : NSControlTextDidChangeNotification;
    NGTextWatch* watch = [[NGTextWatch alloc] init];
    watch.token = [[NSNotificationCenter defaultCenter]
        addObserverForName:name
                    object:source
                     queue:nil
                usingBlock:^(NSNotification* note) {
                    if (!objc_getAssociatedObject(note.object, &kQuietKey)) ng_invoke_custom_callback(change_id);
                }];
    objc_setAssociatedObject(source, &kWatchKey, watch, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    return 1;
}

int ng_macos_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    NSString* text = ng_macos_to_nsstring(content);
    NSTextField* field = field_of(text_handle);
    if (field) {
        [field setStringValue:text];
        return NG_SUCCESS;
    }
    NSTextView* view = text_view_of(text_handle);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    [view setString:text];
    [[view undoManager] removeAllActions];
    return NG_SUCCESS;
}

char* ng_macos_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    NSTextField* field = field_of(text_handle);
    NSTextView* view = text_view_of(text_handle);
    NSString* content = field ? [field stringValue] : view ? [view string] : nil;
    const char* utf8 = content ? [content UTF8String] : NULL;
    return utf8 ? strdup(utf8) : NULL;
}

void ng_macos_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

int ng_macos_text_unit(void) {
    return NG_TEXT_UNIT_UTF16;
}

int ng_macos_text_get_selection(NGHandle text, int* start, int* end) {
    if (!start || !end) return NG_ERROR_INVALID_PARAMETER;
    NSTextView* view = text_view_of(text);
    NSTextField* field = field_of(text);
    NSRange range;
    if (view) {
        range = [view selectedRange];
    } else if (field) {
        NSText* editor = [field currentEditor];
        NSUInteger length = [[field stringValue] length];
        range = editor ? [editor selectedRange] : NSMakeRange(length, 0);
    } else {
        return NG_ERROR_INVALID_HANDLE;
    }
    *start = (int)range.location;
    *end = (int)(range.location + range.length);
    return NG_SUCCESS;
}

static NSUInteger text_length(NGHandle text) {
    NSTextView* view = text_view_of(text);
    return view ? [[view string] length] : [[field_of(text) stringValue] length];
}

int ng_macos_text_set_selection(NGHandle text, int start, int end) {
    NSTextView* view = text_view_of(text);
    NSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || (NSUInteger)end > text_length(text)) return NG_ERROR_INVALID_PARAMETER;
    NSRange range = NSMakeRange((NSUInteger)start, (NSUInteger)(end - start));
    if (view) {
        [view setSelectedRange:range];
        return NG_SUCCESS;
    }
    NSText* editor = [field currentEditor];
    if (!editor) return NG_ERROR_UNSUPPORTED;
    [editor setSelectedRange:range];
    return NG_SUCCESS;
}

int ng_macos_text_replace(NGHandle text, int start, int end, const char* replacement) {
    if (!replacement) return NG_ERROR_INVALID_PARAMETER;
    NSTextView* view = text_view_of(text);
    NSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || (NSUInteger)end > text_length(text)) return NG_ERROR_INVALID_PARAMETER;
    NSRange range = NSMakeRange((NSUInteger)start, (NSUInteger)(end - start));
    NSString* with = ng_macos_to_nsstring(replacement);
    if (field) {
        [field setStringValue:[[field stringValue] stringByReplacingCharactersInRange:range withString:with]];
        return NG_SUCCESS;
    }
    set_quiet(view, YES);
    if ([view shouldChangeTextInRange:range replacementString:with]) {
        [[view textStorage] replaceCharactersInRange:range withString:with];
        [view didChangeText];
        [view setSelectedRange:NSMakeRange(range.location + [with length], 0)];
    }
    set_quiet(view, NO);
    return NG_SUCCESS;
}

int ng_macos_text_undo(NGHandle text, int redo) {
    NSTextView* view = text_view_of(text);
    if (!view) return field_of(text) ? NG_ERROR_UNSUPPORTED : NG_ERROR_INVALID_HANDLE;
    if (!ng_macos_text_can_undo(text, redo)) return NG_ERROR_INVALID_PARAMETER;
    set_quiet(view, YES);
    if (redo) [[view undoManager] redo];
    else [[view undoManager] undo];
    set_quiet(view, NO);
    return NG_SUCCESS;
}

int ng_macos_text_can_undo(NGHandle text, int redo) {
    NSTextView* view = text_view_of(text);
    if (!view || ![view allowsUndo]) return 0;
    NSUndoManager* undo = [view undoManager];
    return (redo ? [undo canRedo] : [undo canUndo]) ? 1 : 0;
}

static void set_wrap(NSTextView* view, BOOL wrap) {
    NSScrollView* scroll = [view enclosingScrollView];
    NSTextContainer* container = [view textContainer];
    [container setWidthTracksTextView:wrap];
    [view setHorizontallyResizable:!wrap];
    [scroll setHasHorizontalScroller:!wrap];
    CGFloat width = wrap ? [scroll contentSize].width : FLT_MAX;
    [container setContainerSize:NSMakeSize(width, FLT_MAX)];
    [view setAutoresizingMask:wrap ? NSViewWidthSizable : NSViewNotSizable];
    if (wrap) [view setFrameSize:NSMakeSize([scroll contentSize].width, [view frame].size.height)];
}

int ng_macos_text_set_feature(NGHandle text, int feature, int on) {
    NSTextView* view = text_view_of(text);
    NSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    switch (feature) {
        case NG_TEXT_FEATURE_READ_ONLY:
            if (view) [view setEditable:!on];
            else [field setEditable:!on];
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_WRAP:
            if (!view) return NG_ERROR_UNSUPPORTED;
            set_wrap(view, on ? YES : NO);
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_MONOSPACE: {
            CGFloat size = [NSFont systemFontSize];
            NSFont* font = [NSFont systemFontOfSize:size];
            if (on) {
                if (@available(macOS 10.15, *)) {
                    font = [NSFont monospacedSystemFontOfSize:size weight:NSFontWeightRegular];
                } else {
                    font = [NSFont userFixedPitchFontOfSize:size];
                }
            }
            if (view) [view setFont:font];
            else [field setFont:font];
            return NG_SUCCESS;
        }
        case NG_TEXT_FEATURE_UNDO:
            if (!view) return NG_ERROR_UNSUPPORTED;
            [view setAllowsUndo:on ? YES : NO];
            if (!on) [[view undoManager] removeAllActions];
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_SECRET: {
            if (![field isKindOfClass:[NSSecureTextField class]]) return NG_ERROR_UNSUPPORTED;
            NSTextFieldCell* old = field.cell;
            if ([old isKindOfClass:[NSSecureTextFieldCell class]] == (on != 0)) return NG_SUCCESS;
            NSString* value = field.stringValue;
            NSText* editor = field.currentEditor;
            NSRange selection = editor ? editor.selectedRange : NSMakeRange(value.length, 0);
            [field abortEditing];
            NSTextFieldCell* cell = on ? [[NSSecureTextFieldCell alloc] initTextCell:value]
                                       : [[NSTextFieldCell alloc] initTextCell:value];
            cell.editable = old.editable;
            cell.selectable = old.selectable;
            cell.bezeled = old.bezeled;
            cell.bezelStyle = old.bezelStyle;
            cell.bordered = old.bordered;
            cell.drawsBackground = old.drawsBackground;
            cell.backgroundColor = old.backgroundColor;
            cell.textColor = old.textColor;
            cell.font = old.font;
            cell.alignment = old.alignment;
            cell.placeholderString = old.placeholderString;
            cell.scrollable = YES;
            cell.usesSingleLineMode = YES;
            cell.target = old.target;
            cell.action = old.action;
            field.cell = cell;
            if (editor && [field.window makeFirstResponder:field]) field.currentEditor.selectedRange = selection;
            [field setNeedsDisplay:YES];
            return NG_SUCCESS;
        }
        default:
            return NG_ERROR_INVALID_PARAMETER;
    }
}
