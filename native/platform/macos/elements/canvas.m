#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <stdlib.h>
#import <string.h>

@interface AureaCanvasView : NSView
// Owned pixel buffer – grown on demand, never per-frame-allocated.
@property (nonatomic, assign) unsigned char* ownedBuffer;
@property (nonatomic, assign) size_t ownedBufferSize;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@property (nonatomic, assign) unsigned int requestedWidth;
@property (nonatomic, assign) unsigned int requestedHeight;
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
    // AutoLayout changed our frame – schedule a repaint so Rust picks up
    // the new bounds on the next check_and_resize pass.
    [self setNeedsDisplay:YES];
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    // Do NOT call ng_process_frames() here – that causes reentrant Rust calls
    // inside AppKit's drawing context and drives a setNeedsDisplay spin-loop.
    // Frame processing is driven by the Rust event loop and the CFRunLoop timer.

    if (self.ownedBuffer && self.bufferWidth > 0 && self.bufferHeight > 0) {
        CGContextRef context = [[NSGraphicsContext currentContext] CGContext];
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();

        unsigned int w = self.bufferWidth;
        unsigned int h = self.bufferHeight;

        // No per-frame malloc: use the owned buffer directly.
        // Buffer is BGRA (u32 ARGB little-endian).
        CGBitmapInfo bitmapInfo = (CGBitmapInfo)kCGImageAlphaFirst
            | kCGBitmapByteOrder32Little;

        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL,
            (const void*)self.ownedBuffer,
            (size_t)w * (size_t)h * 4,
            NULL  // no release – buffer lifetime is managed by the view
        );

        CGImageRef image = CGImageCreate(
            w, h, 8, 32, (size_t)w * 4,
            colorSpace, bitmapInfo, provider,
            NULL, NO, kCGRenderingIntentDefault
        );

        if (image) {
            CGRect viewRect = [self bounds];
            // Our buffer has row 0 at the top; CG draws images bottom-up.
            // Flip the CTM so the image renders right-side-up without copying.
            CGContextSaveGState(context);
            CGContextTranslateCTM(context, 0.0, viewRect.size.height);
            CGContextScaleCTM(context, 1.0, -1.0);
            CGContextDrawImage(context, viewRect, image);
            CGContextRestoreGState(context);
            CGImageRelease(image);
        }

        CGDataProviderRelease(provider);
        CGColorSpaceRelease(colorSpace);
    } else {
        [[NSColor windowBackgroundColor] setFill];
        NSRectFill(dirtyRect);
    }
}

- (void)dealloc {
    free(self.ownedBuffer);
    self.ownedBuffer = NULL;
    self.ownedBufferSize = 0;
}

@end

NGHandle ng_macos_create_canvas(int width, int height) {
    @autoreleasepool {
        AureaCanvasView* canvasView = [[AureaCanvasView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
        [canvasView setWantsLayer:YES];
        canvasView.ownedBuffer = NULL;
        canvasView.ownedBufferSize = 0;
        canvasView.bufferWidth = 0;
        canvasView.bufferHeight = 0;
        canvasView.requestedWidth = (unsigned int)width;
        canvasView.requestedHeight = (unsigned int)height;
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];

        return (__bridge_retained void*)canvasView;
    }
}

void ng_macos_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    NSView* view = (__bridge NSView*)canvas;
    [view setNeedsDisplay:YES];
}

void ng_macos_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    if (!canvas) return;
    NSView* view = (__bridge NSView*)canvas;
    NSRect rect = NSMakeRect(x, y, width, height);
    [view setNeedsDisplayInRect:rect];
}

void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size __attribute__((unused)), unsigned int width, unsigned int height) {
    if (!canvas || !buffer || width == 0 || height == 0) return;

    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) return;

    size_t needed = (size_t)width * (size_t)height * 4;

    // Grow the owned buffer only when the dimensions change – never per-frame.
    if (needed > view.ownedBufferSize) {
        free(view.ownedBuffer);
        view.ownedBuffer = (unsigned char*)malloc(needed);
        view.ownedBufferSize = view.ownedBuffer ? needed : 0;
    }

    if (view.ownedBuffer) {
        memcpy(view.ownedBuffer, buffer, needed);
    }

    view.bufferWidth = width;
    view.bufferHeight = height;
    // Do NOT call setNeedsDisplay here. The caller (ng_macos_canvas_invalidate)
    // schedules the repaint after the buffer is fully written. Calling it here
    // too causes an extra drawRect: on every write, driving a redraw spin-loop.
}

void ng_macos_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;

    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) {
        // Fallback for unknown view types.
        NSRect bounds = [view bounds];
        *width  = (unsigned int)bounds.size.width;
        *height = (unsigned int)bounds.size.height;
        return;
    }

    // Force AutoLayout to finish so we read the post-resize bounds, not the
    // stale pre-layout frame that exists when windowDidResize: fires.
    [view layoutSubtreeIfNeeded];

    NSRect bounds = [view bounds];
    unsigned int w = (unsigned int)bounds.size.width;
    unsigned int h = (unsigned int)bounds.size.height;

    // Fall back to the creation size only if layout hasn't produced valid bounds.
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
