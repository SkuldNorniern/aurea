#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/platform_api.h"
#import <Cocoa/Cocoa.h>
#import <math.h>

/* Wraps at whatever width the layout gives it, so long text grows the label down rather than
   pushing the window wider. */
@interface NGLabel : NSTextField
@property (assign) CGFloat wrapWidth;
@end

@implementation NGLabel
/* As wide as the text on one line; as tall as the text wrapped to the width layout gave it. */
- (NSSize)intrinsicContentSize {
    NSSize natural = [self.cell cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, CGFLOAT_MAX)];
    CGFloat height = natural.height;
    if (_wrapWidth > 0 && _wrapWidth < natural.width) {
        height = [self.cell cellSizeForBounds:NSMakeRect(0, 0, _wrapWidth, CGFLOAT_MAX)].height;
    }
    return NSMakeSize(ceil(natural.width), ceil(height));
}

- (void)setFrameSize:(NSSize)size {
    [super setFrameSize:size];
    if (fabs(size.width - _wrapWidth) >= 0.5) {
        _wrapWidth = size.width;
        [self invalidateIntrinsicContentSize];
    }
}

/* New text is measured on one line until layout gives it a width; wrapping it to the old width
   could grow the window for a moment, and a window doesn't shrink back. */
- (void)textChanged {
    _wrapWidth = 0;
    [self invalidateIntrinsicContentSize];
}
@end

int ng_macos_is_wrapping_label(NSView* view) {
    return [view isKindOfClass:[NGLabel class]];
}

/* An empty label is a spacer; VoiceOver skips a filler. */
static void ng_macos_label_show_text(NSTextField* label, const char* text) {
    label.stringValue = ng_macos_to_nsstring(text) ?: @"";
    label.accessibilityElement = *text != '\0';
}

NGHandle ng_macos_create_label(const char* text) {
    if (!text) return NULL;
    
    NSTextField* label = [[NGLabel alloc] init];
    ng_macos_label_show_text(label, text);
    label.lineBreakMode = NSLineBreakByWordWrapping;
    label.usesSingleLineMode = NO;
    label.maximumNumberOfLines = 0;
    [label.cell setWraps:YES];
    [label setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                    forOrientation:NSLayoutConstraintOrientationHorizontal];
    [label setBezeled:NO];
    [label setDrawsBackground:NO];
    [label setEditable:NO];
    [label setSelectable:NO];
    [label sizeToFit];
    
    return (__bridge_retained void*)label;
}

int ng_macos_label_set_text(NGHandle label, const char* text) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    NSTextField* lbl = (__bridge NSTextField*)label;
    ng_macos_label_show_text(lbl, text);
    if ([lbl isKindOfClass:[NGLabel class]]) [(NGLabel*)lbl textChanged];
    else [lbl invalidateIntrinsicContentSize];
    return NG_SUCCESS;
}

int ng_macos_label_set_style(NGHandle label, int style) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (style < NG_TEXT_STYLE_BODY || style > NG_TEXT_STYLE_HEADING) return NG_ERROR_INVALID_PARAMETER;
    CGFloat size = [NSFont systemFontSize];
    NSFont* font = style == NG_TEXT_STYLE_STRONG ? [NSFont systemFontOfSize:size weight:NSFontWeightSemibold]
                 : style == NG_TEXT_STYLE_SMALL ? [NSFont systemFontOfSize:[NSFont smallSystemFontSize]]
                 : style == NG_TEXT_STYLE_HEADING ? [NSFont systemFontOfSize:size * 1.4 weight:NSFontWeightSemibold]
                                                  : [NSFont systemFontOfSize:size];
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setFont:font];
    if ([lbl isKindOfClass:[NGLabel class]]) [(NGLabel*)lbl textChanged];
    else [lbl invalidateIntrinsicContentSize];
    return NG_SUCCESS;
}

void ng_macos_label_invalidate(NGHandle label) {
    if (!label) return;
    NSTextField* lbl = (__bridge NSTextField*)label;
    [lbl setNeedsDisplay:YES];
}

