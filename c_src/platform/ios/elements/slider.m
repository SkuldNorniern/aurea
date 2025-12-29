#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_slider(double min, double max) {
    if (min >= max) return NULL;
    
    UISlider* slider = [[UISlider alloc] init];
    [slider setMinimumValue:min];
    [slider setMaximumValue:max];
    [slider setValue:(min + max) / 2.0];
    [slider setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)slider;
}

int ng_ios_slider_set_value(NGHandle slider, double value) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    UISlider* uiSlider = (__bridge UISlider*)slider;
    float minValue = [uiSlider minimumValue];
    float maxValue = [uiSlider maximumValue];
    
    if (value < minValue) value = minValue;
    if (value > maxValue) value = maxValue;
    
    [uiSlider setValue:(float)value];
    return NG_SUCCESS;
}

double ng_ios_slider_get_value(NGHandle slider) {
    if (!slider) return 0.0;
    
    UISlider* uiSlider = (__bridge UISlider*)slider;
    return (double)[uiSlider value];
}

int ng_ios_slider_set_enabled(NGHandle slider, int enabled) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    UISlider* uiSlider = (__bridge UISlider*)slider;
    [uiSlider setEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

