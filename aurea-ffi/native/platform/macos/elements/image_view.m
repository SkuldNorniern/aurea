#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/image_fit.h"
#import <Cocoa/Cocoa.h>

@interface NGImageView : NSView
@property (strong) NSImage* image;
@property (assign) int scaling;
@end

@implementation NGImageView

- (BOOL)isFlipped {
    return YES;
}

- (NSSize)intrinsicContentSize {
    return self.image ? self.image.size : NSMakeSize(NSViewNoIntrinsicMetric, NSViewNoIntrinsicMetric);
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    if (!self.image) return;
    NSRect bounds = self.bounds;
    NSSize size = self.image.size;
    NGImageRect r = ng_image_fit(self.scaling, bounds.size.width, bounds.size.height,
                                 size.width, size.height);
    [NSGraphicsContext saveGraphicsState];
    NSRectClip(bounds);
    [self.image drawInRect:NSMakeRect(r.x, r.y, r.width, r.height)
                  fromRect:NSZeroRect
                 operation:NSCompositingOperationSourceOver
                  fraction:1.0
            respectFlipped:YES
                     hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
    [NSGraphicsContext restoreGraphicsState];
}

- (void)showImage:(NSImage*)image {
    self.image = image;
    [self invalidateIntrinsicContentSize];
    [self setNeedsDisplay:YES];
}

@end

NGHandle ng_macos_create_image_view(void) {
    NGImageView* imageView = [[NGImageView alloc] initWithFrame:NSZeroRect];
    imageView.scaling = NG_IMAGE_SCALING_ASPECT_FIT;
    [imageView setTranslatesAutoresizingMaskIntoConstraints:NO];
    for (int o = 0; o < 2; o++) {
        NSLayoutConstraintOrientation axis = (NSLayoutConstraintOrientation)o;
        [imageView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:axis];
        [imageView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:axis];
    }
    return (__bridge_retained void*)imageView;
}

int ng_macos_image_view_load_from_path(NGHandle image_view, const char* path) {
    if (!image_view || !path) return NG_ERROR_INVALID_PARAMETER;
    NSImage* image = [[NSImage alloc] initWithContentsOfFile:ng_macos_to_nsstring(path)];
    if (!image) return NG_ERROR_CREATION_FAILED;
    [(__bridge NGImageView*)image_view showImage:image];
    return NG_SUCCESS;
}

int ng_macos_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    if (!image_view || !data || size == 0) return NG_ERROR_INVALID_PARAMETER;
    NSImage* image = [[NSImage alloc] initWithData:[NSData dataWithBytes:data length:size]];
    if (!image) return NG_ERROR_CREATION_FAILED;
    [(__bridge NGImageView*)image_view showImage:image];
    return NG_SUCCESS;
}

void ng_macos_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    if (!image_view) return;
    NGImageView* imageView = (__bridge NGImageView*)image_view;
    imageView.scaling = scaling_mode;
    [imageView setNeedsDisplay:YES];
}


int ng_macos_image_view_get_size(NGHandle image_view, unsigned int* width, unsigned int* height) {
    if (!image_view || !width || !height) return NG_ERROR_INVALID_PARAMETER;
    NSImage* image = ((__bridge NGImageView*)image_view).image;
    if (!image) return NG_ERROR_INVALID_HANDLE;
    NSInteger w = 0;
    NSInteger h = 0;
    for (NSImageRep* rep in image.representations) {
        if (rep.pixelsWide * rep.pixelsHigh > w * h) {
            w = rep.pixelsWide;
            h = rep.pixelsHigh;
        }
    }
    if (w <= 0 || h <= 0) {
        w = (NSInteger)image.size.width;
        h = (NSInteger)image.size.height;
    } else if ((w > h) != (image.size.width > image.size.height)) {
        NSInteger turned = w;
        w = h;
        h = turned;
    }
    *width = (unsigned int)w;
    *height = (unsigned int)h;
    return NG_SUCCESS;
}

void ng_macos_image_view_invalidate(NGHandle image_view) {
    if (!image_view) return;
    [(__bridge NGImageView*)image_view setNeedsDisplay:YES];
}
