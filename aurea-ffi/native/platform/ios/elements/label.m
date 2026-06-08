#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_label_impl(const char* text) {
    if (!text) return NULL;
    
    UILabel* label = [[UILabel alloc] init];
    label.text = ng_ios_to_nsstring(text);
    label.textAlignment = NSTextAlignmentCenter;
    [label sizeToFit];
    
    return (__bridge_retained void*)label;
}



