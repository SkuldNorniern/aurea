#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:4.0];
    // Columns line children up at the leading edge; rows center them, so a
    // label sits level with the button beside it.
    [stack setAlignment:is_vertical ? NSLayoutAttributeLeading : NSLayoutAttributeCenterY];
    // Children pack from the start and space they leave stays empty at the
    // end. With Fill, every fixed child hugged equally hard, so which one
    // took the window's extra space was left to chance.
    [stack setDistribution:NSStackViewDistributionGravityAreas];
    // Weighted children (250) give way first and grow; fixed ones (750)
    // keep their size. Below the window keeping its size (500), so a stack
    // never shrinks the window to fit.
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

    // A child grows when it is weighted, or when it asks to itself: canvases,
    // scroll views and spacers hug loosely both ways, where labels, buttons
    // and sliders hug their height. A nested stack is its children's size
    // unless weighted. A growing child fills across the stack too; a fixed
    // one keeps its natural size from the leading (or centered) edge.
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

    // A growing child, and a stack in a row, also hold the leading (or top)
    // edge: the stack's alignment alone lets a view with no width of its
    // own, like a split view, slide to the far side. A stack in a row runs
    // the row's height, so a column beside a canvas starts at the top.
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

    // A nested stack sizes itself by its own hugging, not content hugging:
    // a fixed one stays the size of its children along this stack's axis.
    // Across it, a stretched one gives way below a split view's holding
    // priority (250), or it would pull the split's pane down to its children.
    if (nested) {
        NSLayoutConstraintOrientation axis = vertical ? NSLayoutConstraintOrientationVertical
                                                      : NSLayoutConstraintOrientationHorizontal;
        NSLayoutConstraintOrientation across = vertical ? NSLayoutConstraintOrientationHorizontal
                                                        : NSLayoutConstraintOrientationVertical;
        [(NSStackView*)view setHuggingPriority:fills ? NSLayoutPriorityDefaultLow : NSLayoutPriorityDefaultHigh
                                forOrientation:axis];
        if (stretched) [(NSStackView*)view setHuggingPriority:200 forOrientation:across];
    }
    
    /* Weight > 0: expand to fill. Use low hugging so the view can grow; keep compression
     * resistance high so views with intrinsic size (e.g. canvas) are not shrunk below it. */
    if (fills) {
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];
    } else {
        // Fixed/natural size elements have higher hugging priority
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

