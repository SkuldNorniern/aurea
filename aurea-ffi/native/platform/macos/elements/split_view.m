#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

@interface AureaSplitViewDelegate : NSObject <NSSplitViewDelegate>
@end

@implementation AureaSplitViewDelegate
- (BOOL)splitView:(NSSplitView *)splitView canCollapseSubview:(NSView *)subview {
    return NO;
}

- (CGFloat)splitView:(NSSplitView *)splitView constrainMinCoordinate:(CGFloat)proposedMinimumPosition ofSubviewAt:(NSInteger)dividerIndex {
    return proposedMinimumPosition + 50.0;
}

- (CGFloat)splitView:(NSSplitView *)splitView constrainMaxCoordinate:(CGFloat)proposedMaximumPosition ofSubviewAt:(NSInteger)dividerIndex {
    return proposedMaximumPosition - 50.0;
}
@end

@interface AureaSplitView : NSSplitView
@property (nonatomic, strong) NSMutableDictionary<NSNumber*, NSNumber*>* pendingPositions;
- (void)applyPosition:(CGFloat)position ofDividerAtIndex:(NSInteger)index;
@end

@implementation AureaSplitView
- (BOOL)hasSize {
    NSSize size = self.bounds.size;
    return (self.isVertical ? size.width : size.height) > 0.0;
}

- (void)applyPosition:(CGFloat)position ofDividerAtIndex:(NSInteger)index {
    if (![self hasSize]) {
        if (!self.pendingPositions) self.pendingPositions = [NSMutableDictionary dictionary];
        self.pendingPositions[@(index)] = @(position);
        self.needsLayout = YES;
        return;
    }
    [self setPosition:position ofDividerAtIndex:index];
}

- (void)layout {
    [super layout];
    if (self.pendingPositions.count == 0 || ![self hasSize]) return;
    NSDictionary<NSNumber*, NSNumber*>* pending = self.pendingPositions;
    self.pendingPositions = nil;
    for (NSNumber* index in [pending.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        [self setPosition:pending[index].doubleValue ofDividerAtIndex:index.integerValue];
    }
}
@end

static AureaSplitViewDelegate* splitViewDelegate = nil;

NGHandle ng_macos_create_split_view(int is_vertical) {
    if (!splitViewDelegate) {
        splitViewDelegate = [[AureaSplitViewDelegate alloc] init];
    }
    
    NSSplitView* split = [[AureaSplitView alloc] init];
    [split setVertical:!is_vertical];
    [split setDividerStyle:NSSplitViewDividerStyleThin];
    [split setDelegate:splitViewDelegate];
    [split setArrangesAllSubviews:YES];
    
    return (__bridge_retained void*)split;
}

int ng_macos_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSSplitView* split = (__bridge NSSplitView*)split_handle;
    NSView* view = (__bridge NSView*)element;
    
    [view setTranslatesAutoresizingMaskIntoConstraints:NO];
    [split addArrangedSubview:view];
    if ([view isKindOfClass:[NSStackView class]]) {
        [(NSStackView*)view setHuggingPriority:200 forOrientation:NSLayoutConstraintOrientationHorizontal];
        [(NSStackView*)view setHuggingPriority:200 forOrientation:NSLayoutConstraintOrientationVertical];
    }
    
    [split adjustSubviews];
    
    return NG_SUCCESS;
}

int ng_macos_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    
    AureaSplitView* split = (__bridge AureaSplitView*)split_handle;
    
    dispatch_async(dispatch_get_main_queue(), ^{
        [split applyPosition:position ofDividerAtIndex:index];
    });
    
    return NG_SUCCESS;
}
