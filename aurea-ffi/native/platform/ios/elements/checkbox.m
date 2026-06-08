#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_checkbox(const char* label) {
    UISwitch* switchControl = [[UISwitch alloc] init];
    [switchControl setOn:NO];
    [switchControl setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    if (label) {
        UILabel* labelView = [[UILabel alloc] init];
        [labelView setText:[NSString stringWithUTF8String:label]];
        [labelView setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        UIStackView* container = [[UIStackView alloc] init];
        [container setAxis:UILayoutConstraintAxisHorizontal];
        [container setSpacing:8.0];
        [container addArrangedSubview:labelView];
        [container addArrangedSubview:switchControl];
        [container setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        return (__bridge_retained void*)container;
    }
    
    return (__bridge_retained void*)switchControl;
}

int ng_ios_checkbox_set_checked(NGHandle checkbox, int checked) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    UIView* view = (__bridge UIView*)checkbox;
    UISwitch* switchControl = nil;
    
    if ([view isKindOfClass:[UISwitch class]]) {
        switchControl = (UISwitch*)view;
    } else if ([view isKindOfClass:[UIStackView class]]) {
        UIStackView* stackView = (UIStackView*)view;
        for (UIView* subview in [stackView arrangedSubviews]) {
            if ([subview isKindOfClass:[UISwitch class]]) {
                switchControl = (UISwitch*)subview;
                break;
            }
        }
    }
    
    if (switchControl) {
        [switchControl setOn:checked ? YES : NO];
        return NG_SUCCESS;
    }
    
    return NG_ERROR_INVALID_HANDLE;
}

int ng_ios_checkbox_get_checked(NGHandle checkbox) {
    if (!checkbox) return 0;
    
    UIView* view = (__bridge UIView*)checkbox;
    UISwitch* switchControl = nil;
    
    if ([view isKindOfClass:[UISwitch class]]) {
        switchControl = (UISwitch*)view;
    } else if ([view isKindOfClass:[UIStackView class]]) {
        UIStackView* stackView = (UIStackView*)view;
        for (UIView* subview in [stackView arrangedSubviews]) {
            if ([subview isKindOfClass:[UISwitch class]]) {
                switchControl = (UISwitch*)subview;
                break;
            }
        }
    }
    
    if (switchControl) {
        return [switchControl isOn] ? 1 : 0;
    }
    
    return 0;
}

int ng_ios_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    UIView* view = (__bridge UIView*)checkbox;
    UISwitch* switchControl = nil;
    
    if ([view isKindOfClass:[UISwitch class]]) {
        switchControl = (UISwitch*)view;
    } else if ([view isKindOfClass:[UIStackView class]]) {
        UIStackView* stackView = (UIStackView*)view;
        for (UIView* subview in [stackView arrangedSubviews]) {
            if ([subview isKindOfClass:[UISwitch class]]) {
                switchControl = (UISwitch*)subview;
                break;
            }
        }
    }
    
    if (switchControl) {
        [switchControl setEnabled:enabled ? YES : NO];
        return NG_SUCCESS;
    }
    
    return NG_ERROR_INVALID_HANDLE;
}



