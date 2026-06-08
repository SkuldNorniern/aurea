#import "../elements.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_progress_bar(void) {
    NSProgressIndicator* progressBar = [[NSProgressIndicator alloc] init];
    [progressBar setStyle:NSProgressIndicatorStyleBar];
    [progressBar setIndeterminate:NO];
    [progressBar setMinValue:0.0];
    [progressBar setMaxValue:1.0];
    [progressBar setDoubleValue:0.0];
    [progressBar setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)progressBar;
}

int ng_macos_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    NSProgressIndicator* progressBar = (__bridge NSProgressIndicator*)progress_bar;
    
    if (value < 0.0) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    [progressBar setIndeterminate:NO];
    [progressBar setDoubleValue:value];
    [progressBar setNeedsDisplay:YES];
    return NG_SUCCESS;
}

int ng_macos_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    NSProgressIndicator* progressBar = (__bridge NSProgressIndicator*)progress_bar;
    [progressBar setIndeterminate:indeterminate ? YES : NO];
    
    if (indeterminate) {
        [progressBar startAnimation:nil];
    } else {
        [progressBar stopAnimation:nil];
    }
    
    return NG_SUCCESS;
}

int ng_macos_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    NSProgressIndicator* progressBar = (__bridge NSProgressIndicator*)progress_bar;
    [progressBar setControlSize:enabled ? NSControlSizeRegular : NSControlSizeSmall];
    return NG_SUCCESS;
}

void ng_macos_progress_bar_invalidate(NGHandle progress_bar) {
    if (!progress_bar) return;
    NSProgressIndicator* progressBar = (__bridge NSProgressIndicator*)progress_bar;
    [progressBar setNeedsDisplay:YES];
}

