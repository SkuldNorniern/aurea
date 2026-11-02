#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/platform_api.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>
#include <stdlib.h>
#include <string.h>

/* The text elements: a one-line UITextField, and a UITextView for editors and text views. Offsets
   are UTF-16, as NSString counts them. Changes the app makes are not reported. */

@interface NGIOSTextField : UITextField <UITextFieldDelegate>
@property (assign) BOOL readOnly;
@end

@implementation NGIOSTextField
- (BOOL)textField:(UITextField*)field shouldChangeCharactersInRange:(NSRange)range replacementString:(NSString*)text {
    (void)field; (void)range; (void)text;
    return !_readOnly;
}

- (BOOL)textFieldShouldReturn:(UITextField*)field {
    [field resignFirstResponder];
    return YES;
}
@end

@interface NGIOSTextView : UITextView <UITextViewDelegate>
@property (assign) unsigned int changeId;
@property (assign) BOOL watched;
@property (assign) BOOL quiet;
@property (assign) BOOL allowsUndo;
@end

@implementation NGIOSTextView
- (void)textViewDidChange:(UITextView*)view {
    (void)view;
    if (_watched && !_quiet) ng_invoke_custom_callback(_changeId);
}

- (NSUndoManager*)undoManager {
    return _allowsUndo ? [super undoManager] : nil;
}
@end

static NGIOSTextField* field_of(NGHandle text) {
    id view = (__bridge id)text;
    return [view isKindOfClass:[NGIOSTextField class]] ? view : nil;
}

static NGIOSTextView* view_of(NGHandle text) {
    id view = (__bridge id)text;
    return [view isKindOfClass:[NGIOSTextView class]] ? view : nil;
}

static NSString* text_of(NGHandle text) {
    NGIOSTextView* view = view_of(text);
    return view ? view.text : field_of(text).text;
}

NGHandle ng_platform_create_text_field(void) {
    NGIOSTextField* field = [[NGIOSTextField alloc] init];
    field.borderStyle = UITextBorderStyleRoundedRect;
    field.delegate = field;
    return (__bridge_retained void*)field;
}

NGHandle ng_platform_create_search_field(void) {
    NGIOSTextField* field = [[NGIOSTextField alloc] init];
    field.borderStyle = UITextBorderStyleRoundedRect;
    field.delegate = field;
    field.clearButtonMode = UITextFieldViewModeWhileEditing;
    field.returnKeyType = UIReturnKeySearch;
    if (@available(iOS 13.0, *)) {
        UIImageView* mark = [[UIImageView alloc] initWithImage:[UIImage systemImageNamed:@"magnifyingglass"]];
        mark.tintColor = UIColor.secondaryLabelColor;
        field.leftView = mark;
        field.leftViewMode = UITextFieldViewModeAlways;
    }
    return (__bridge_retained void*)field;
}

int ng_platform_text_set_placeholder(NGHandle text, const char* placeholder) {
    UITextField* field = field_of(text);
    if (!field) return NG_ERROR_UNSUPPORTED;
    if (!placeholder) return NG_ERROR_INVALID_PARAMETER;
    field.placeholder = [NSString stringWithUTF8String:placeholder];
    return NG_SUCCESS;
}

static NGHandle create_text_view(BOOL editable) {
    NGIOSTextView* view = [[NGIOSTextView alloc] init];
    view.editable = editable;
    view.allowsUndo = YES;
    view.delegate = view;
    view.font = [UIFont systemFontOfSize:UIFont.systemFontSize];
    return (__bridge_retained void*)view;
}

NGHandle ng_platform_create_text_editor(void) {
    return create_text_view(YES);
}

NGHandle ng_platform_create_text_view(int is_editable) {
    return create_text_view(is_editable != 0);
}

int ng_platform_set_text_content(NGHandle text, const char* content) {
    if (!text) return NG_ERROR_INVALID_HANDLE;
    if (!content) return NG_ERROR_INVALID_PARAMETER;
    NSString* value = [NSString stringWithUTF8String:content];
    if (!value) return NG_ERROR_INVALID_PARAMETER;
    NGIOSTextView* view = view_of(text);
    NGIOSTextField* field = field_of(text);
    if (view) view.text = value;
    else if (field) field.text = value;
    else return NG_ERROR_INVALID_HANDLE;
    return NG_SUCCESS;
}

char* ng_platform_get_text_content(NGHandle text) {
    if (!text) return NULL;
    NSString* value = text_of(text);
    return value ? strdup(value.UTF8String) : NULL;
}

int ng_ios_text_watch_change(NGHandle text, unsigned int id) {
    NGIOSTextView* view = view_of(text);
    if (!view) return 0;
    view.changeId = id;
    view.watched = YES;
    return 1;
}

int ng_platform_text_get_selection(NGHandle text, int* start, int* end) {
    if (!start || !end) return NG_ERROR_INVALID_PARAMETER;
    NGIOSTextView* view = view_of(text);
    NGIOSTextField* field = field_of(text);
    NSRange range;
    if (view) {
        range = view.selectedRange;
    } else if (field) {
        UITextRange* selected = field.selectedTextRange;
        if (!selected) {
            range = NSMakeRange(field.text.length, 0);
        } else {
            NSInteger from = [field offsetFromPosition:field.beginningOfDocument toPosition:selected.start];
            NSInteger to = [field offsetFromPosition:field.beginningOfDocument toPosition:selected.end];
            range = NSMakeRange((NSUInteger)from, (NSUInteger)(to - from));
        }
    } else {
        return NG_ERROR_INVALID_HANDLE;
    }
    *start = (int)range.location;
    *end = (int)(range.location + range.length);
    return NG_SUCCESS;
}

int ng_platform_text_set_selection(NGHandle text, int start, int end) {
    NGIOSTextView* view = view_of(text);
    NGIOSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || (NSUInteger)end > text_of(text).length) return NG_ERROR_INVALID_PARAMETER;
    if (view) {
        view.selectedRange = NSMakeRange((NSUInteger)start, (NSUInteger)(end - start));
        return NG_SUCCESS;
    }
    UITextPosition* from = [field positionFromPosition:field.beginningOfDocument offset:start];
    UITextPosition* to = [field positionFromPosition:field.beginningOfDocument offset:end];
    field.selectedTextRange = [field textRangeFromPosition:from toPosition:to];
    return NG_SUCCESS;
}

int ng_platform_text_replace(NGHandle text, int start, int end, const char* replacement) {
    if (!replacement) return NG_ERROR_INVALID_PARAMETER;
    NGIOSTextView* view = view_of(text);
    NGIOSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    if (start < 0 || end < start || (NSUInteger)end > text_of(text).length) return NG_ERROR_INVALID_PARAMETER;
    NSString* with = [NSString stringWithUTF8String:replacement];
    if (!with) return NG_ERROR_INVALID_PARAMETER;
    NSRange range = NSMakeRange((NSUInteger)start, (NSUInteger)(end - start));
    if (field) {
        field.text = [field.text stringByReplacingCharactersInRange:range withString:with];
        return NG_SUCCESS;
    }
    view.quiet = YES;
    UITextPosition* from = [view positionFromPosition:view.beginningOfDocument offset:start];
    UITextPosition* to = [view positionFromPosition:view.beginningOfDocument offset:end];
    [view replaceRange:[view textRangeFromPosition:from toPosition:to] withText:with];
    view.quiet = NO;
    return NG_SUCCESS;
}

int ng_platform_text_can_undo(NGHandle text, int redo) {
    NGIOSTextView* view = view_of(text);
    if (!view || !view.allowsUndo) return 0;
    NSUndoManager* undo = view.undoManager;
    return (redo ? undo.canRedo : undo.canUndo) ? 1 : 0;
}

int ng_platform_text_undo(NGHandle text, int redo) {
    NGIOSTextView* view = view_of(text);
    if (!view) return field_of(text) ? NG_ERROR_UNSUPPORTED : NG_ERROR_INVALID_HANDLE;
    if (!ng_platform_text_can_undo(text, redo)) return NG_ERROR_INVALID_PARAMETER;
    view.quiet = YES;
    if (redo) [view.undoManager redo];
    else [view.undoManager undo];
    view.quiet = NO;
    return NG_SUCCESS;
}

int ng_platform_text_set_feature(NGHandle text, int feature, int on) {
    NGIOSTextView* view = view_of(text);
    NGIOSTextField* field = field_of(text);
    if (!view && !field) return NG_ERROR_INVALID_HANDLE;
    switch (feature) {
        case NG_TEXT_FEATURE_READ_ONLY:
            if (view) view.editable = !on;
            else field.readOnly = on != 0;
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_WRAP:
            if (!view) return NG_ERROR_UNSUPPORTED;
            view.textContainer.widthTracksTextView = on != 0;
            view.textContainer.size = CGSizeMake(on ? view.bounds.size.width : CGFLOAT_MAX, CGFLOAT_MAX);
            view.alwaysBounceHorizontal = !on;
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_MONOSPACE: {
            CGFloat size = UIFont.systemFontSize;
            UIFont* font = [UIFont systemFontOfSize:size];
            if (on) {
                if (@available(iOS 13.0, *)) {
                    font = [UIFont monospacedSystemFontOfSize:size weight:UIFontWeightRegular];
                } else {
                    font = [UIFont fontWithName:@"Menlo" size:size] ?: font;
                }
            }
            if (view) view.font = font;
            else field.font = font;
            return NG_SUCCESS;
        }
        case NG_TEXT_FEATURE_UNDO:
            if (!view) return NG_ERROR_UNSUPPORTED;
            if (!on) [view.undoManager removeAllActions];
            view.allowsUndo = on != 0;
            return NG_SUCCESS;
        case NG_TEXT_FEATURE_SECRET:
            if (!field) return NG_ERROR_UNSUPPORTED;
            field.secureTextEntry = on != 0;
            return NG_SUCCESS;
        default:
            return NG_ERROR_INVALID_PARAMETER;
    }
}
