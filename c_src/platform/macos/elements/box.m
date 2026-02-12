#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:4.0];
    // Use Leading/Top as anchor for cross-axis pinning
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
    
    // Cross-axis pinning to ensure elements fill the stack view's thickness
    if ([stack orientation] == NSUserInterfaceLayoutOrientationVertical) {
        // In vertical stack, pin trailing edge to fill width
        [view.trailingAnchor constraintEqualToAnchor:stack.trailingAnchor constant:-stack.edgeInsets.right].active = YES;
    } else {
        // In horizontal stack, pin bottom edge to fill height
        [view.bottomAnchor constraintEqualToAnchor:stack.bottomAnchor constant:-stack.edgeInsets.bottom].active = YES;
    }
    
    /* Weight > 0: expand to fill. Use low hugging so the view can grow; keep compression
     * resistance high so views with intrinsic size (e.g. canvas) are not shrunk below it. */
    if (weight > 0.0f) {
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

