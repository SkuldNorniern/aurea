#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/platform_api.h"
#import <Cocoa/Cocoa.h>

@interface AureaScrollDocument : NSView
@end

@implementation AureaScrollDocument
- (BOOL)isFlipped {
    return YES;
}
@end

NGHandle ng_macos_create_scroll_view(void) {
    NSScrollView* scroll = [[NSScrollView alloc] init];
    scroll.hasVerticalScroller = YES;
    scroll.hasHorizontalScroller = NO;
    scroll.autohidesScrollers = YES;
    scroll.drawsBackground = NO;

    AureaScrollDocument* document = [[AureaScrollDocument alloc] init];
    document.translatesAutoresizingMaskIntoConstraints = NO;
    scroll.documentView = document;
    NSClipView* clip = scroll.contentView;
    [NSLayoutConstraint activateConstraints:@[
        [document.leadingAnchor constraintEqualToAnchor:clip.leadingAnchor],
        [document.trailingAnchor constraintEqualToAnchor:clip.trailingAnchor],
        [document.topAnchor constraintEqualToAnchor:clip.topAnchor],
    ]];
    return (__bridge_retained void*)scroll;
}

int ng_macos_scroll_view_set_content(NGHandle scroll_handle, NGHandle element) {
    if (!scroll_handle || !element) return NG_ERROR_INVALID_HANDLE;
    NSScrollView* scroll = (__bridge NSScrollView*)scroll_handle;
    NSView* content = (__bridge NSView*)element;
    NSView* document = scroll.documentView;
    if (!document) return NG_ERROR_PLATFORM_SPECIFIC;
    if (content.superview == document) return NG_SUCCESS;

    for (NSView* old in [document.subviews copy]) [old removeFromSuperview];
    content.translatesAutoresizingMaskIntoConstraints = NO;
    [document addSubview:content];
    [NSLayoutConstraint activateConstraints:@[
        [content.leadingAnchor constraintEqualToAnchor:document.leadingAnchor],
        [content.trailingAnchor constraintEqualToAnchor:document.trailingAnchor],
        [content.topAnchor constraintEqualToAnchor:document.topAnchor],
        [content.bottomAnchor constraintEqualToAnchor:document.bottomAnchor],
    ]];
    [document scrollPoint:NSZeroPoint];
    return NG_SUCCESS;
}

int ng_macos_scroll_view_set_scrollbar(NGHandle scroll_handle, int policy) {
    if (!scroll_handle) return NG_ERROR_INVALID_HANDLE;
    if (policy != NG_SCROLLBAR_AUTO && policy != NG_SCROLLBAR_ALWAYS) return NG_ERROR_INVALID_PARAMETER;
    NSScrollView* scroll = (__bridge NSScrollView*)scroll_handle;
    scroll.autohidesScrollers = policy == NG_SCROLLBAR_AUTO;
    scroll.scrollerStyle = policy == NG_SCROLLBAR_ALWAYS ? NSScrollerStyleLegacy : [NSScroller preferredScrollerStyle];
    return NG_SUCCESS;
}
