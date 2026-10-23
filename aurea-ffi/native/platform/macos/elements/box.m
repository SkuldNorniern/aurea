#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:4.0];
    [stack setAlignment:is_vertical ? NSLayoutAttributeLeading : NSLayoutAttributeCenterY];
    [stack setDistribution:NSStackViewDistributionGravityAreas];
    [stack setHuggingPriority:300
               forOrientation:is_vertical ? NSLayoutConstraintOrientationVertical
                                          : NSLayoutConstraintOrientationHorizontal];
    [stack setEdgeInsets:NSEdgeInsetsMake(4.0, 4.0, 4.0, 4.0)];
    
    return (__bridge_retained void*)stack;
}

int ng_macos_box_add(NGHandle box, NGHandle element, float weight) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSStackView* stack = (__bridge NSStackView*)box;
    NSView* view = (__bridge NSView*)element;
    
    [view setTranslatesAutoresizingMaskIntoConstraints:NO];
    [stack addView:view inGravity:NSStackViewGravityLeading];

    BOOL vertical = [stack orientation] == NSUserInterfaceLayoutOrientationVertical;
    BOOL asksToGrow = ![view isKindOfClass:[NSStackView class]]
        && [view contentHuggingPriorityForOrientation:NSLayoutConstraintOrientationHorizontal] <= NSLayoutPriorityDefaultLow
        && [view contentHuggingPriorityForOrientation:NSLayoutConstraintOrientationVertical] <= NSLayoutPriorityDefaultLow;
    BOOL fills = weight > 0.0f || asksToGrow;
    NSLayoutConstraint* cross;
    if (vertical) {
        NSLayoutXAxisAnchor* edge = stack.trailingAnchor;
        CGFloat inset = -stack.edgeInsets.right;
        cross = fills ? [view.trailingAnchor constraintEqualToAnchor:edge constant:inset]
                      : [view.trailingAnchor constraintLessThanOrEqualToAnchor:edge constant:inset];
    } else {
        NSLayoutYAxisAnchor* edge = stack.bottomAnchor;
        CGFloat inset = -stack.edgeInsets.bottom;
        cross = fills ? [view.bottomAnchor constraintEqualToAnchor:edge constant:inset]
                      : [view.bottomAnchor constraintLessThanOrEqualToAnchor:edge constant:inset];
    }
    cross.active = YES;

    BOOL nested = [view isKindOfClass:[NSStackView class]];
    BOOL stretched = fills || (!vertical && nested);
    if (stretched) {
        if (vertical) {
            [view.leadingAnchor constraintEqualToAnchor:stack.leadingAnchor
                                               constant:stack.edgeInsets.left].active = YES;
        } else {
            [view.topAnchor constraintEqualToAnchor:stack.topAnchor
                                           constant:stack.edgeInsets.top].active = YES;
            if (!fills) {
                [view.bottomAnchor constraintEqualToAnchor:stack.bottomAnchor
                                                  constant:-stack.edgeInsets.bottom].active = YES;
            }
        }
    }

    if (nested) {
        NSLayoutConstraintOrientation axis = vertical ? NSLayoutConstraintOrientationVertical
                                                      : NSLayoutConstraintOrientationHorizontal;
        NSLayoutConstraintOrientation across = vertical ? NSLayoutConstraintOrientationHorizontal
                                                        : NSLayoutConstraintOrientationVertical;
        [(NSStackView*)view setHuggingPriority:fills ? NSLayoutPriorityDefaultLow : NSLayoutPriorityDefaultHigh
                                forOrientation:axis];
        if (stretched) [(NSStackView*)view setHuggingPriority:200 forOrientation:across];
    }
    
    if (fills) {
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];
    } else {
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
        /* A label wraps instead of widening the window. */
        NSLayoutPriority squeeze = ng_macos_is_wrapping_label(view) ? NSLayoutPriorityDefaultLow : NSLayoutPriorityDefaultHigh;
        [view setContentCompressionResistancePriority:squeeze forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
    }
    
    return NG_SUCCESS;
}

void ng_macos_box_invalidate(NGHandle box_handle) {
    if (!box_handle) return;
    NSStackView* stack = (__bridge NSStackView*)box_handle;
    [stack setNeedsDisplay:YES];
}

