#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

extern void ng_invoke_button_callback(unsigned int id);

@interface ButtonTarget : NSObject
- (void)buttonClicked:(id)sender;
@end

@implementation ButtonTarget
- (void)buttonClicked:(id)sender {
    UIButton* button = (UIButton*)sender;
    unsigned int id = (unsigned int)button.tag;
    ng_invoke_button_callback(id);
}
@end

static ButtonTarget* buttonTarget = nil;

NGHandle ng_ios_create_button_impl(const char* title, unsigned int id) {
    if (!title) return NULL;
    
    if (!buttonTarget) {
        buttonTarget = [[ButtonTarget alloc] init];
    }
    
    NSString* nsTitle = ng_ios_to_nsstring(title);
    UIButton* button = [UIButton buttonWithType:UIButtonTypeSystem];
    [button setTitle:nsTitle forState:UIControlStateNormal];
    [button addTarget:buttonTarget action:@selector(buttonClicked:) forControlEvents:UIControlEventTouchUpInside];
    [button setTag:id];
    [button sizeToFit];
    
    return (__bridge_retained void*)button;
}

