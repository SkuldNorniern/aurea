#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:4.0];
    [stack setAlignment:is_vertical ? NSLayoutAttributeLeading : NSLayoutAttributeTop];
    [stack setDistribution:NSStackViewDistributionFill];
    [stack setEdgeInsets:NSEdgeInsetsMake(4.0, 4.0, 4.0, 4.0)];
    
    return (__bridge_retained void*)stack;
}

int ng_macos_box_add(NGHandle box, NGHandle element, float weight) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSStackView* stack = (__bridge NSStackView*)box;
    NSView* view = (__bridge NSView*)element;
    
    [view setTranslatesAutoresizingMaskIntoConstraints:NO];
    [stack addArrangedSubview:view];
    
    if ([stack orientation] == NSUserInterfaceLayoutOrientationVertical) {
        [view.trailingAnchor constraintEqualToAnchor:stack.trailingAnchor constant:-stack.edgeInsets.right].active = YES;
    } else {
        [view.bottomAnchor constraintEqualToAnchor:stack.bottomAnchor constant:-stack.edgeInsets.bottom].active = YES;
    }
    
    if (weight > 0.0f) {
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];
    } else {
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
    }
    
    return NG_SUCCESS;
}

void ng_macos_box_invalidate(NGHandle box_handle) {
    if (!box_handle) return;
    NSStackView* stack = (__bridge NSStackView*)box_handle;
    [stack setNeedsDisplay:YES];
}

