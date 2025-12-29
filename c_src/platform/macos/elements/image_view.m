#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

// Scaling mode constants
#define IMAGE_SCALING_NONE 0
#define IMAGE_SCALING_ASPECT_FIT 1
#define IMAGE_SCALING_ASPECT_FILL 2
#define IMAGE_SCALING_FILL 3

NGHandle ng_macos_create_image_view(void) {
    NSImageView* imageView = [[NSImageView alloc] init];
    [imageView setImageScaling:NSImageScaleProportionallyUpOrDown];
    [imageView setImageAlignment:NSImageAlignCenter];
    [imageView setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    // Set maximum height constraint to make image display smaller
    NSLayoutConstraint* heightConstraint = [NSLayoutConstraint constraintWithItem:imageView
                                                                        attribute:NSLayoutAttributeHeight
                                                                        relatedBy:NSLayoutRelationLessThanOrEqual
                                                                           toItem:nil
                                                                        attribute:NSLayoutAttributeNotAnAttribute
                                                                       multiplier:1.0
                                                                         constant:200.0];
    [imageView addConstraint:heightConstraint];
    
    return (__bridge_retained void*)imageView;
}

int ng_macos_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    
    NSImageView* imageView = (__bridge NSImageView*)image_view;
    NSString* nsPath = ng_macos_to_nsstring(path);
    NSImage* image = [[NSImage alloc] initWithContentsOfFile:nsPath];
    
    if (!image) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    [imageView setImage:image];
    return NG_SUCCESS;
}

int ng_macos_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;
    
    NSImageView* imageView = (__bridge NSImageView*)image_view;
    NSData* imageData = [NSData dataWithBytes:data length:size];
    NSImage* image = [[NSImage alloc] initWithData:imageData];
    
    if (!image) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    [imageView setImage:image];
    return NG_SUCCESS;
}

void ng_macos_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    
    NSImageView* imageView = (__bridge NSImageView*)image_view;
    
    switch (scaling_mode) {
        case IMAGE_SCALING_NONE:
            [imageView setImageScaling:NSImageScaleNone];
            break;
        case IMAGE_SCALING_ASPECT_FIT:
            [imageView setImageScaling:NSImageScaleProportionallyUpOrDown];
            break;
        case IMAGE_SCALING_ASPECT_FILL:
            [imageView setImageScaling:NSImageScaleProportionallyUpOrDown];
            [imageView setImageAlignment:NSImageAlignCenter];
            break;
        case IMAGE_SCALING_FILL:
            [imageView setImageScaling:NSImageScaleAxesIndependently];
            break;
        default:
            [imageView setImageScaling:NSImageScaleProportionallyUpOrDown];
            break;
    }
}

void ng_macos_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    NSImageView* imageView = (__bridge NSImageView*)image_view;
    [imageView setNeedsDisplay:YES];
}

