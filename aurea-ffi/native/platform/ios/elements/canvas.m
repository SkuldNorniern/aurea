#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

@interface AureaCanvasView : UIView
@end
@implementation AureaCanvasView
- (void)dealloc {
    ng_invoke_canvas_destroyed((__bridge void*)self);
}
@end

NGHandle ng_ios_create_canvas_impl(int width, int height) {
    UIView* canvasView = [[AureaCanvasView alloc] initWithFrame:CGRectMake(0, 0, width, height)];
    canvasView.backgroundColor = [UIColor whiteColor];
    
    return (__bridge_retained void*)canvasView;
}



