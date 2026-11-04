#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#import <IOSurface/IOSurface.h>
#import <CoreVideo/CVPixelBuffer.h>
#import <stdlib.h>
#import <string.h>
#import <math.h>

// Seconds without a frame before the surface off screen is freed.
static const CFTimeInterval NG_IDLE_SURFACE_SECONDS = 2.0;

@interface AureaCanvasView : NSView
{
@public
    IOSurfaceRef _ioSurfaces[2];
    int _ioBackIndex;
    unsigned int _ioSurfWidth;
    unsigned int _ioSurfHeight;
    CGFloat _lastContentsScale;
    CFTimeInterval _lastPresent;
    BOOL _idleCheckPending;
}
- (void)releaseSurfaces;
- (void)checkIdleAfter:(CFTimeInterval)delay;
@property (nonatomic, assign) const unsigned char* renderBuffer;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@property (nonatomic, assign) unsigned int requestedWidth;
@property (nonatomic, assign) unsigned int requestedHeight;
@property (nonatomic, assign) unsigned int cachedWidth;
@property (nonatomic, assign) unsigned int cachedHeight;
@property (nonatomic, assign) BOOL gpuOwned;
@end

@implementation AureaCanvasView

- (BOOL)isFlipped {
    return YES;
}

- (NSSize)intrinsicContentSize {
    return NSMakeSize((CGFloat)self.requestedWidth, (CGFloat)self.requestedHeight);
}

- (void)layout {
    [super layout];
    NSSize size = self.bounds.size;
    unsigned int width = (unsigned int)ceil(size.width);
    unsigned int height = (unsigned int)ceil(size.height);
    if (width != self.cachedWidth || height != self.cachedHeight) {
        self.cachedWidth = width;
        self.cachedHeight = height;
        [self setNeedsDisplay:YES];
        ng_invoke_canvas_resized((__bridge void*)self);
    }
}

- (BOOL)wantsUpdateLayer {
    return YES;
}

- (void)updateLayer {
    self.layer.backgroundColor = self.gpuOwned ? nil : [[NSColor windowBackgroundColor] CGColor];
}

- (void)dealloc {
    ng_invoke_canvas_destroyed((__bridge void*)self);
    self.renderBuffer = NULL;
    [self releaseSurfaces];
}

- (void)releaseSurfaces {
    for (int i = 0; i < 2; i++) {
        if (_ioSurfaces[i]) {
            CFRelease(_ioSurfaces[i]);
            _ioSurfaces[i] = NULL;
        }
    }
}

- (void)checkIdleAfter:(CFTimeInterval)delay {
    __weak AureaCanvasView* weakSelf = self;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        AureaCanvasView* view = weakSelf;
        if (!view) return;
        CFTimeInterval left = view->_lastPresent + NG_IDLE_SURFACE_SECONDS - CACurrentMediaTime();
        if (left > 0.0) {
            [view checkIdleAfter:left];
            return;
        }
        view->_idleCheckPending = NO;
        IOSurfaceRef back = view->_ioSurfaces[view->_ioBackIndex];
        if (back) {
            CFRelease(back);
            view->_ioSurfaces[view->_ioBackIndex] = NULL;
        }
    });
}

@end

NGHandle ng_macos_create_canvas(int width, int height) {
    @autoreleasepool {
        AureaCanvasView* canvasView = [[AureaCanvasView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
        [canvasView setWantsLayer:YES];
        canvasView.renderBuffer = NULL;
        canvasView.bufferWidth = 0;
        canvasView.bufferHeight = 0;
        canvasView.requestedWidth = (unsigned int)width;
        canvasView.requestedHeight = (unsigned int)height;
        canvasView.cachedWidth  = (unsigned int)width;
        canvasView.cachedHeight = (unsigned int)height;
        canvasView.gpuOwned = NO;
        canvasView->_lastContentsScale = 0.0;
        canvasView.layer.opaque = YES;
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];

        return (__bridge_retained void*)canvasView;
    }
}

void ng_macos_canvas_invalidate(NGHandle canvas) {
    (void)canvas;
}

void ng_macos_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    (void)canvas;
    (void)x;
    (void)y;
    (void)width;
    (void)height;
}

void ng_macos_canvas_set_gpu_owned(NGHandle canvas, int gpu_owned) {
    if (!canvas) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;
    view.gpuOwned = gpu_owned != 0;
    [view setNeedsDisplay:YES];
    if (view.gpuOwned) {
        view.renderBuffer = NULL;
        view.bufferWidth = 0;
        view.bufferHeight = 0;
        view.layer.contents = nil;
        [view releaseSurfaces];
    }
}

static CGColorSpaceRef s_canvas_colorspace = NULL;

void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size __attribute__((unused)), unsigned int width, unsigned int height) {
    if (!canvas || !buffer || width == 0 || height == 0) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    view.renderBuffer = buffer;
    view.bufferWidth  = width;
    view.bufferHeight = height;

    if (!s_canvas_colorspace) {
        s_canvas_colorspace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    }

    CFDataRef pixels = CFDataCreate(NULL, buffer, (CFIndex)((size_t)width * height * 4));
    if (!pixels) return;
    CGDataProviderRef provider = CGDataProviderCreateWithCFData(pixels);
    CFRelease(pixels);
    CGImageRef image = CGImageCreate(width, height, 8, 32, (size_t)width * 4,
        s_canvas_colorspace,
        (CGBitmapInfo)kCGImageAlphaFirst | kCGBitmapByteOrder32Little,
        provider, NULL, NO, kCGRenderingIntentDefault);
    CGDataProviderRelease(provider);
    if (!image) return;

    view.layer.contents = (__bridge id)image;
    CGFloat boundsWidth = view.bounds.size.width;
    CGFloat newScale = (boundsWidth > 0.0) ? (CGFloat)width / boundsWidth : 1.0;
    if (newScale != view->_lastContentsScale) {
        view.layer.contentsScale = newScale;
        view->_lastContentsScale = newScale;
    }
    CGImageRelease(image);
}

static IOSurfaceRef ng_macos_create_io_surface(unsigned int width, unsigned int height) {
    NSDictionary* props = @{
        (id)kIOSurfaceWidth: @(width),
        (id)kIOSurfaceHeight: @(height),
        (id)kIOSurfaceBytesPerElement: @4,
        (id)kIOSurfacePixelFormat: @(kCVPixelFormatType_32BGRA),
    };
    IOSurfaceRef surface = IOSurfaceCreate((CFDictionaryRef)props);
    static CFPropertyListRef srgb = NULL;
    if (!srgb) {
        CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        srgb = space ? CGColorSpaceCopyPropertyList(space) : NULL;
        CGColorSpaceRelease(space);
    }
    if (surface && srgb) IOSurfaceSetValue(surface, CFSTR("IOSurfaceColorSpace"), srgb);
    return surface;
}

static void ng_macos_copy_io_surface(IOSurfaceRef from, IOSurfaceRef to, unsigned int height) {
    IOSurfaceLock(from, kIOSurfaceLockReadOnly, NULL);
    size_t from_row = IOSurfaceGetBytesPerRow(from);
    size_t to_row = IOSurfaceGetBytesPerRow(to);
    size_t row = from_row < to_row ? from_row : to_row;
    const unsigned char* src = (const unsigned char*)IOSurfaceGetBaseAddress(from);
    unsigned char* dst = (unsigned char*)IOSurfaceGetBaseAddress(to);
    for (unsigned int y = 0; y < height; y++) {
        memcpy(dst + y * to_row, src + y * from_row, row);
    }
    IOSurfaceUnlock(from, kIOSurfaceLockReadOnly, NULL);
}

void* ng_macos_canvas_acquire_buffer(NGHandle canvas, unsigned int width, unsigned int height, unsigned int* stride_px, unsigned int* buffer_index) {
    if (!canvas || width == 0 || height == 0) return NULL;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return NULL;

    if (view->_ioSurfWidth != width || view->_ioSurfHeight != height) {
        [view releaseSurfaces];
        view->_ioSurfWidth = width;
        view->_ioSurfHeight = height;
        view->_ioBackIndex = 0;
    }

    IOSurfaceRef back = view->_ioSurfaces[view->_ioBackIndex];
    IOSurfaceRef from = NULL;
    if (!back) {
        back = ng_macos_create_io_surface(width, height);
        if (!back) return NULL;
        view->_ioSurfaces[view->_ioBackIndex] = back;
        from = view->_ioSurfaces[1 - view->_ioBackIndex];
    }

    IOSurfaceLock(back, 0, NULL);

    // Damage only covers what changed since this surface was last drawn.
    if (from) ng_macos_copy_io_surface(from, back, height);

    if (stride_px) *stride_px = (unsigned int)(IOSurfaceGetBytesPerRow(back) / 4);
    if (buffer_index) *buffer_index = (unsigned int)view->_ioBackIndex;

    return (unsigned char*)IOSurfaceGetBaseAddress(back);
}

void ng_macos_canvas_present(NGHandle canvas) {
    if (!canvas) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    IOSurfaceRef back = view->_ioSurfaces[view->_ioBackIndex];
    if (!back) return;

    IOSurfaceUnlock(back, 0, NULL);

    view.layer.contents = (__bridge id)back;
    CGFloat boundsWidth = view.bounds.size.width;
    CGFloat newScale = (boundsWidth > 0.0) ? (CGFloat)view->_ioSurfWidth / boundsWidth : 1.0;
    if (newScale != view->_lastContentsScale) {
        view.layer.contentsScale = newScale;
        view->_lastContentsScale = newScale;
    }

    view->_ioBackIndex = 1 - view->_ioBackIndex;

    view->_lastPresent = CACurrentMediaTime();
    if (!view->_idleCheckPending) {
        view->_idleCheckPending = YES;
        [view checkIdleAfter:NG_IDLE_SURFACE_SECONDS];
    }
}

void ng_macos_canvas_get_pixel_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;
    NSView* view = (__bridge NSView*)canvas;
    NSSize pixels = [view convertSizeToBacking:[view bounds].size];
    *width = (unsigned int)lround(pixels.width);
    *height = (unsigned int)lround(pixels.height);
}

void ng_macos_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;

    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) {
        NSRect bounds = [view bounds];
        *width  = (unsigned int)bounds.size.width;
        *height = (unsigned int)bounds.size.height;
        return;
    }

    unsigned int w = view.cachedWidth;
    unsigned int h = view.cachedHeight;
    if (w == 0) w = view.requestedWidth;
    if (h == 0) h = view.requestedHeight;

    *width  = w;
    *height = h;
}

NGHandle ng_macos_canvas_get_window(NGHandle canvas) {
    if (!canvas) return NULL;
    NSView* view = (__bridge NSView*)canvas;
    NSWindow* window = [view window];
    return (__bridge void*)window;
}

NGHandle ng_macos_canvas_get_native_handle(NGHandle canvas) {
    if (!canvas) return NULL;
    NSView* view = (__bridge NSView*)canvas;
    return (__bridge void*)view;
}
