#import "../elements.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_slider(double min, double max) {
    if (min >= max) return NULL;
    
    NSSlider* slider = [[NSSlider alloc] init];
    [slider setMinValue:min];
    [slider setMaxValue:max];
    [slider setDoubleValue:(min + max) / 2.0];
    [slider setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)slider;
}

int ng_macos_slider_set_value(NGHandle slider, double value) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    NSSlider* nsSlider = (__bridge NSSlider*)slider;
    double minValue = [nsSlider minValue];
    double maxValue = [nsSlider maxValue];
    
    if (value < minValue) value = minValue;
    if (value > maxValue) value = maxValue;
    
    [nsSlider setDoubleValue:value];
    return NG_SUCCESS;
}

double ng_macos_slider_get_value(NGHandle slider) {
    if (!slider) return 0.0;
    
    NSSlider* nsSlider = (__bridge NSSlider*)slider;
    return [nsSlider doubleValue];
}

int ng_macos_slider_set_enabled(NGHandle slider, int enabled) {
    if (!slider) return NG_ERROR_INVALID_HANDLE;
    
    NSSlider* nsSlider = (__bridge NSSlider*)slider;
    [nsSlider setEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

void ng_macos_slider_invalidate(NGHandle slider) {
    if (!slider) return;
    NSSlider* nsSlider = (__bridge NSSlider*)slider;
    [nsSlider setNeedsDisplay:YES];
}



