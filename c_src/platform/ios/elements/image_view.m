#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

// Scaling mode constants
#define IMAGE_SCALING_NONE 0
#define IMAGE_SCALING_ASPECT_FIT 1
#define IMAGE_SCALING_ASPECT_FILL 2
#define IMAGE_SCALING_FILL 3

NGHandle ng_ios_create_image_view(void) {
    UIImageView* imageView = [[UIImageView alloc] init];
    [imageView setContentMode:UIViewContentModeScaleAspectFit];
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

int ng_ios_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    
    UIImageView* imageView = (__bridge UIImageView*)image_view;
    NSString* nsPath = [NSString stringWithUTF8String:path];
    UIImage* image = [UIImage imageWithContentsOfFile:nsPath];
    
    if (!image) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    [imageView setImage:image];
    return NG_SUCCESS;
}

int ng_ios_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;
    
    UIImageView* imageView = (__bridge UIImageView*)image_view;
    NSData* imageData = [NSData dataWithBytes:data length:size];
    UIImage* image = [UIImage imageWithData:imageData];
    
    if (!image) {
        return NG_ERROR_CREATION_FAILED;
    }
    
    [imageView setImage:image];
    return NG_SUCCESS;
}

void ng_ios_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    
    UIImageView* imageView = (__bridge UIImageView*)image_view;
    
    switch (scaling_mode) {
        case IMAGE_SCALING_NONE:
            [imageView setContentMode:UIViewContentModeCenter];
            break;
        case IMAGE_SCALING_ASPECT_FIT:
            [imageView setContentMode:UIViewContentModeScaleAspectFit];
            break;
        case IMAGE_SCALING_ASPECT_FILL:
            [imageView setContentMode:UIViewContentModeScaleAspectFill];
            break;
        case IMAGE_SCALING_FILL:
            [imageView setContentMode:UIViewContentModeScaleToFill];
            break;
        default:
            [imageView setContentMode:UIViewContentModeScaleAspectFit];
            break;
    }
}

