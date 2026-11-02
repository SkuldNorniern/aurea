#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/platform_api.h"
#import <UIKit/UIKit.h>

/* Scrolls one element, as wide as the view and as tall as it wants. */

NGHandle ng_platform_create_scroll_view(void) {
    UIScrollView* scroll = [[UIScrollView alloc] init];
    scroll.showsHorizontalScrollIndicator = NO;
    return (__bridge_retained void*)scroll;
}

int ng_platform_scroll_view_set_content(NGHandle scroll_handle, NGHandle element) {
    if (!scroll_handle || !element) return NG_ERROR_INVALID_HANDLE;
    UIScrollView* scroll = (__bridge UIScrollView*)scroll_handle;
    UIView* content = (__bridge UIView*)element;
    if (content.superview == scroll) return NG_SUCCESS;
    for (UIView* old in [scroll.subviews copy]) [old removeFromSuperview];
    content.translatesAutoresizingMaskIntoConstraints = NO;
    [scroll addSubview:content];
    [NSLayoutConstraint activateConstraints:@[
        [content.leadingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.leadingAnchor],
        [content.trailingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.trailingAnchor],
        [content.topAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.topAnchor],
        [content.bottomAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.bottomAnchor],
        [content.widthAnchor constraintEqualToAnchor:scroll.frameLayoutGuide.widthAnchor],
    ]];
    scroll.contentOffset = CGPointZero;
    return NG_SUCCESS;
}

int ng_platform_scroll_view_set_scrollbar(NGHandle scroll_handle, int policy) {
    if (!scroll_handle) return NG_ERROR_INVALID_HANDLE;
    if (policy != NG_SCROLLBAR_AUTO && policy != NG_SCROLLBAR_ALWAYS) return NG_ERROR_INVALID_PARAMETER;
    UIScrollView* scroll = (__bridge UIScrollView*)scroll_handle;
    /* iOS only shows its bar while scrolling; ALWAYS shows it now, too. */
    scroll.showsVerticalScrollIndicator = YES;
    if (policy == NG_SCROLLBAR_ALWAYS) [scroll flashScrollIndicators];
    return NG_SUCCESS;
}
