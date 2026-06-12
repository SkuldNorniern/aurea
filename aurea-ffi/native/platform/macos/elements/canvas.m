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

@interface AureaCanvasView : NSView
{
@public
    // IOSurface double-buffer pair for the acquire/present presentation
    // path (P5-A). NULL until the first ng_macos_canvas_acquire_buffer
    // call; recreated whenever the requested size changes. The layer
    // retains its own reference to whichever surface is assigned to
    // `layer.contents`, so releasing our reference here on resize never
    // invalidates what CoreAnimation is currently compositing.
    IOSurfaceRef _ioSurfaces[2];
    int _ioBackIndex;
    unsigned int _ioSurfWidth;
    unsigned int _ioSurfHeight;
}
// Raw pointer into Rust's frame_buffer.  Never freed here — Rust owns it.
// Safe: everything runs on the main thread; ng_macos_canvas_update_buffer
// reads it synchronously to build the CGImage assigned to layer.contents.
@property (nonatomic, assign) const unsigned char* renderBuffer;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@property (nonatomic, assign) unsigned int requestedWidth;
@property (nonatomic, assign) unsigned int requestedHeight;
// Last size recorded by -layout; avoids layoutSubtreeIfNeeded on every frame.
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
        // Cache the new size so canvas_get_size can read it without forcing
        // another layout pass.
        self.cachedWidth  = (unsigned int)newSize.width;
        self.cachedHeight = (unsigned int)newSize.height;
        [self setNeedsDisplay:YES];
    }
}

// Only used before the first frame buffer arrives — once
// ng_macos_canvas_update_buffer runs, presentation goes through
// view.layer.contents directly and this is never called again.
- (void)drawRect:(NSRect)dirtyRect {
    [[NSColor windowBackgroundColor] setFill];
    NSRectFill(dirtyRect);
}

- (void)dealloc {
    // renderBuffer is owned by Rust — do not free.
    self.renderBuffer = NULL;
    for (int i = 0; i < 2; i++) {
        if (_ioSurfaces[i]) {
            CFRelease(_ioSurfaces[i]);
            _ioSurfaces[i] = NULL;
        }
    }
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
        // Seed the cache with the creation dimensions; -layout will update them.
        canvasView.cachedWidth  = (unsigned int)width;
        canvasView.cachedHeight = (unsigned int)height;
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];

        return (__bridge_retained void*)canvasView;
    }
}

// Presentation now goes through view.layer.contents (set in
// ng_macos_canvas_update_buffer), which takes effect immediately — no
// separate invalidation pass is needed.
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

// Shared across all canvases — device RGB never changes at runtime.
static CGColorSpaceRef s_canvas_colorspace = NULL;

void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size __attribute__((unused)), unsigned int width, unsigned int height) {
    if (!canvas || !buffer || width == 0 || height == 0) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    // Keep the raw pointer/size around for bookkeeping (e.g. get_size fallback).
    view.renderBuffer = buffer;
    view.bufferWidth  = width;
    view.bufferHeight = height;

    if (!s_canvas_colorspace) {
        s_canvas_colorspace = CGColorSpaceCreateDeviceRGB();
    }

    // Buffer is BGRA (u32 ARGB little-endian). No copy — wraps Rust's
    // frame_buffer directly. Safe as long as the buffer outlives the image;
    // the image is released at the end of this call and any reference held
    // by the layer is replaced wholesale on the next update.
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

static IOSurfaceRef ng_macos_create_io_surface(unsigned int width, unsigned int height) {
    NSDictionary* props = @{
        (id)kIOSurfaceWidth: @(width),
        (id)kIOSurfaceHeight: @(height),
        (id)kIOSurfaceBytesPerElement: @4,
        (id)kIOSurfacePixelFormat: @(kCVPixelFormatType_32BGRA),
    };
    return IOSurfaceCreate((CFDictionaryRef)props);
}

// Locks and returns the back surface's base address, (re)creating the
// surface pair first if the requested size changed. The pair persists
// across calls (and across resizes the layer doesn't need releasing for —
// see the dealloc comment on _ioSurfaces) so present() can flip between
// the two without reallocating every frame.
void* ng_macos_canvas_acquire_buffer(NGHandle canvas, unsigned int width, unsigned int height, unsigned int* stride_px, unsigned int* buffer_index) {
    if (!canvas || width == 0 || height == 0) return NULL;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return NULL;

    if (view->_ioSurfWidth != width || view->_ioSurfHeight != height || !view->_ioSurfaces[0] || !view->_ioSurfaces[1]) {
        for (int i = 0; i < 2; i++) {
            if (view->_ioSurfaces[i]) {
                CFRelease(view->_ioSurfaces[i]);
                view->_ioSurfaces[i] = NULL;
            }
        }
        view->_ioSurfaces[0] = ng_macos_create_io_surface(width, height);
        view->_ioSurfaces[1] = ng_macos_create_io_surface(width, height);
        view->_ioSurfWidth = width;
        view->_ioSurfHeight = height;
        view->_ioBackIndex = 0;
    }

    IOSurfaceRef back = view->_ioSurfaces[view->_ioBackIndex];
    if (!back) return NULL;

    IOSurfaceLock(back, 0, NULL);

    if (stride_px) *stride_px = (unsigned int)(IOSurfaceGetBytesPerRow(back) / 4);
    if (buffer_index) *buffer_index = (unsigned int)view->_ioBackIndex;

    return (unsigned char*)IOSurfaceGetBaseAddress(back);
}

// Unlocks the back surface acquired above, assigns it to layer.contents
// (CoreAnimation takes its own retain on it — see the ivar comment on
// AureaCanvasView), and flips _ioBackIndex for the next frame.
void ng_macos_canvas_present(NGHandle canvas) {
    if (!canvas) return;
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    IOSurfaceRef back = view->_ioSurfaces[view->_ioBackIndex];
    if (!back) return;

    IOSurfaceUnlock(back, 0, NULL);

    view.layer.contents = (__bridge id)back;
    CGFloat boundsWidth = view.bounds.size.width;
    view.layer.contentsScale = (boundsWidth > 0.0) ? (CGFloat)view->_ioSurfWidth / boundsWidth : 1.0;

    view->_ioBackIndex = 1 - view->_ioBackIndex;
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

    // Read from the ivar cache updated by -layout instead of forcing a
    // synchronous layout subtree pass on every frame.
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
