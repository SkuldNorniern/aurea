#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

NGHandle ng_ios_create_progress_bar(void) {
    UIProgressView* progressBar = [[UIProgressView alloc] initWithProgressViewStyle:UIProgressViewStyleDefault];
    [progressBar setProgress:0.0];
    [progressBar setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)progressBar;
}

int ng_ios_progress_bar_set_value(NGHandle progress_bar, double value) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    if (value < 0.0) value = 0.0;
    if (value > 1.0) value = 1.0;
    
    UIProgressView* progressBar = (__bridge UIProgressView*)progress_bar;
    // Use animated update for smoother transitions during animation
    [progressBar setProgress:(float)value animated:YES];
    [progressBar setNeedsLayout];
    
    return NG_SUCCESS;
}

int ng_ios_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    UIProgressView* progressBar = (__bridge UIProgressView*)progress_bar;
    
    if (indeterminate) {
        [progressBar setProgress:0.5];
        [progressBar setProgressViewStyle:UIProgressViewStyleDefault];
    } else {
        [progressBar setProgressViewStyle:UIProgressViewStyleDefault];
    }
    
    return NG_SUCCESS;
}

int ng_ios_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    if (!progress_bar) return NG_ERROR_INVALID_HANDLE;
    
    UIProgressView* progressBar = (__bridge UIProgressView*)progress_bar;
    [progressBar setUserInteractionEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

