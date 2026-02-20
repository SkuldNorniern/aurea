#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>
#import <stdlib.h>
#import <string.h>

static void releaseFlippedBuffer(void* info, const void* data, size_t size) {
    (void)info;
    (void)size;
    free((void*)data);
}

@interface AureaCanvasView : NSView
@property (nonatomic, assign) unsigned char* renderBuffer;
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
    [self setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
    [self setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];
}

- (void)drawRect:(NSRect)dirtyRect {
    ng_process_frames();

    if (self.renderBuffer && self.bufferWidth > 0 && self.bufferHeight > 0) {
        CGContextRef context = [[NSGraphicsContext currentContext] CGContext];
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();

        unsigned int w = self.bufferWidth;
        unsigned int h = self.bufferHeight;
        size_t rowBytes = (size_t)w * 4;
        size_t byteCount = rowBytes * (size_t)h;

        /* CGContextDrawImage draws image row 0 at the bottom of the rect (CG y-up).
         * Our buffer has row 0 = top. Flip rows so image row 0 = our bottom; then
         * our top appears at the top of the view. */
        unsigned char* flipped = (unsigned char*)malloc(byteCount);
        if (flipped) {
            const unsigned char* src = self.renderBuffer;
            for (unsigned int row = 0; row < h; row++) {
                unsigned int srcRow = h - 1 - row;
                memcpy(flipped + row * rowBytes, src + srcRow * rowBytes, rowBytes);
            }
        }

        const void* dataToUse = flipped ? flipped : (const void*)self.renderBuffer;
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL,
            (void*)dataToUse,
            byteCount,
            flipped ? releaseFlippedBuffer : NULL
        );

        /* Buffer is BGRA (u32 ARGB little-endian): alpha in high byte, then R,G,B. */
        CGBitmapInfo bitmapInfo = (CGBitmapInfo)kCGImageAlphaFirst
            | kCGBitmapByteOrder32Little;

        CGImageRef image = CGImageCreate(
            w,
            h,
            8,
            32,
            (size_t)w * 4,
            colorSpace,
            bitmapInfo,
            provider,
            NULL,
            NO,
            kCGRenderingIntentDefault
        );

        if (image) {
            CGRect viewRect = [self bounds];
            CGContextDrawImage(context, CGRectMake(0, 0, viewRect.size.width, viewRect.size.height), image);
            CGImageRelease(image);
        }

        CGDataProviderRelease(provider);
        CGColorSpaceRelease(colorSpace);
    } else {
        [[NSColor whiteColor] setFill];
        NSRectFill(dirtyRect);
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
        [canvasView setContentHuggingPriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentHuggingPriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationHorizontal];
        [canvasView setContentCompressionResistancePriority:NSLayoutPriorityRequired forOrientation:NSLayoutConstraintOrientationVertical];

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
    if (!canvas || !buffer) return;
    
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if ([view isKindOfClass:[AureaCanvasView class]]) {
        view.renderBuffer = (unsigned char*)buffer;
        view.bufferWidth = width;
        view.bufferHeight = height;
        [view setNeedsDisplay:YES];
    }
}

void ng_macos_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    if (!canvas || !width || !height) return;

    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if (![view isKindOfClass:[AureaCanvasView class]]) {
        NSRect bounds = [view bounds];
        *width = (unsigned int)bounds.size.width;
        *height = (unsigned int)bounds.size.height;
        return;
    }
    NSRect bounds = [view bounds];
    unsigned int w = (unsigned int)bounds.size.width;
    unsigned int h = (unsigned int)bounds.size.height;
    unsigned int reqW = view.requestedWidth;
    unsigned int reqH = view.requestedHeight;
    if (w == 0) w = reqW;
    if (h == 0) h = reqH;
    if (w < reqW) w = reqW;
    if (h < reqH) h = reqH;
    *width = w;
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
