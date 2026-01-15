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

static AureaSplitViewDelegate* splitViewDelegate = nil;

NGHandle ng_macos_create_split_view(int is_vertical) {
    if (!splitViewDelegate) {
        splitViewDelegate = [[AureaSplitViewDelegate alloc] init];
    }
    
    NSSplitView* split = [[NSSplitView alloc] init];
    [split setVertical:!is_vertical]; // NSSplitView uses 'vertical' to mean a vertical divider (horizontal split)
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
    
    [split adjustSubviews];
    
    return NG_SUCCESS;
}

int ng_macos_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    
    NSSplitView* split = (__bridge NSSplitView*)split_handle;
    
    // Ensure we run on main thread if possible, or just call directly since UI usually is on main
    dispatch_async(dispatch_get_main_queue(), ^{
        [split setPosition:position ofDividerAtIndex:index];
    });
    
    return NG_SUCCESS;
}
