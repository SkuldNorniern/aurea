#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_canvas_impl(int width, int height) {
    UIView* canvasView = [[UIView alloc] initWithFrame:CGRectMake(0, 0, width, height)];
    canvasView.backgroundColor = [UIColor whiteColor];
    
    return (__bridge_retained void*)canvasView;
}

