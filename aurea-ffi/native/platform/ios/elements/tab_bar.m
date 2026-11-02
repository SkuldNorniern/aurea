#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

/* Tabs as a segmented control. Tabs can't be dragged out on iOS, so detach never fires. */

@interface NGIOSTabBar : UISegmentedControl
@property (assign) unsigned int tabBarId;
@end

@implementation NGIOSTabBar
- (void)changed {
    ng_invoke_tab_bar_selected(_tabBarId, (int)self.selectedSegmentIndex);
}
@end

NGHandle ng_platform_create_tab_bar(unsigned int id) {
    NGIOSTabBar* bar = [[NGIOSTabBar alloc] init];
    bar.tabBarId = id;
    [bar addTarget:bar action:@selector(changed) forControlEvents:UIControlEventValueChanged];
    return (__bridge_retained void*)bar;
}

int ng_platform_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;
    NGIOSTabBar* bar = (__bridge NGIOSTabBar*)tab_bar;
    [bar insertSegmentWithTitle:@(title) atIndex:bar.numberOfSegments animated:NO];
    if (bar.selectedSegmentIndex == UISegmentedControlNoSegment) bar.selectedSegmentIndex = 0;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    NGIOSTabBar* bar = (__bridge NGIOSTabBar*)tab_bar;
    if (index < 0 || index >= (int)bar.numberOfSegments) return NG_ERROR_INVALID_PARAMETER;
    NSInteger selected = bar.selectedSegmentIndex;
    [bar removeSegmentAtIndex:index animated:NO];
    if (selected >= index && selected > 0) selected--;
    if (bar.numberOfSegments > 0) bar.selectedSegmentIndex = selected;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_set_selected(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    NGIOSTabBar* bar = (__bridge NGIOSTabBar*)tab_bar;
    if (index < 0 || index >= (int)bar.numberOfSegments) return NG_ERROR_INVALID_PARAMETER;
    bar.selectedSegmentIndex = index;
    return NG_SUCCESS;
}

int ng_platform_tab_bar_get_selected(NGHandle tab_bar) {
    if (!tab_bar) return -1;
    NSInteger selected = ((__bridge NGIOSTabBar*)tab_bar).selectedSegmentIndex;
    return selected == UISegmentedControlNoSegment ? -1 : (int)selected;
}

void ng_platform_tab_bar_invalidate(NGHandle tab_bar) {
    if (tab_bar) [(__bridge UIView*)tab_bar setNeedsDisplay];
}
