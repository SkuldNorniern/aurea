#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_box(int is_vertical) {
    UIStackView* stackView = [[UIStackView alloc] init];
    stackView.axis = is_vertical ? UILayoutConstraintAxisVertical : UILayoutConstraintAxisHorizontal;
    stackView.spacing = 8.0;
    stackView.alignment = UIStackViewAlignmentCenter;
    stackView.distribution = UIStackViewDistributionFill;
    
    return (__bridge_retained void*)stackView;
}

int ng_ios_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    UIStackView* stackView = (__bridge UIStackView*)box;
    UIView* view = (__bridge UIView*)element;
    
    [stackView addArrangedSubview:view];
    
    return NG_SUCCESS;
}

