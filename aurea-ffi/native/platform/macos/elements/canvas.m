#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#import <stdlib.h>
#import <string.h>

@interface AureaCanvasView : NSView
@property (nonatomic, assign) const unsigned char* renderBuffer;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@property (nonatomic, assign) unsigned int requestedWidth;
@property (nonatomic, assign) unsigned int requestedHeight;
@property (nonatomic, assign) unsigned int cachedWidth;
@property (nonatomic, assign) unsigned int cachedHeight;
@end

@implementation AureaCanvasView

- (BOOL)isFlipped {
    return YES;
}

- (NSSize)intrinsicContentSize {
    return NSMakeSize((CGFloat)self.requestedWidth, (CGFloat)self.requestedHeight);
}

- (void)layout {
    NSSize oldSize = self.bounds.size;
    [super layout];
    NSSize newSize = self.bounds.size;
    if (!NSEqualSizes(oldSize, newSize)) {
        self.cachedWidth  = (unsigned int)newSize.width;
        self.cachedHeight = (unsigned int)newSize.height;
        [self setNeedsDisplay:YES];
    }
}

- (void)drawRect:(NSRect)dirtyRect {
    [[NSColor windowBackgroundColor] setFill];
    NSRectFill(dirtyRect);
}

- (void)dealloc {
    self.renderBuffer = NULL;
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

static CGColorSpaceRef s_canvas_colorspace = NULL;

void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size __attribute__((unused)), unsigned int width, unsigned int height) {
    if (!canvas || !buffer || width == 0 || height == 0) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    view.renderBuffer = buffer;
    view.bufferWidth  = width;
    view.bufferHeight = height;

    if (!s_canvas_colorspace) {
        s_canvas_colorspace = CGColorSpaceCreateDeviceRGB();
    }

    CGDataProviderRef provider = CGDataProviderCreateWithData(
        NULL, (const void*)buffer, (size_t)width * height * 4, NULL);
    CGImageRef image = CGImageCreate(width, height, 8, 32, (size_t)width * 4,
        s_canvas_colorspace,
        (CGBitmapInfo)kCGImageAlphaFirst | kCGBitmapByteOrder32Little,
        provider, NULL, NO, kCGRenderingIntentDefault);
    CGDataProviderRelease(provider);
    if (!image) return;

    view.layer.contents = (__bridge id)image;
    CGFloat boundsWidth = view.bounds.size.width;
    view.layer.contentsScale = (boundsWidth > 0.0) ? (CGFloat)width / boundsWidth : 1.0;
    CGImageRelease(image);
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
