#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

@interface AureaCanvasView : NSView
@property (nonatomic, assign) unsigned char* renderBuffer;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@end

@implementation AureaCanvasView
- (void)drawRect:(NSRect)dirtyRect {
    if (self.renderBuffer && self.bufferWidth > 0 && self.bufferHeight > 0) {
        CGContextRef context = [[NSGraphicsContext currentContext] CGContext];
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL,
            self.renderBuffer,
            self.bufferWidth * self.bufferHeight * 4,
            NULL
        );
        
        CGBitmapInfo bitmapInfo = (CGBitmapInfo)kCGImageAlphaPremultipliedLast;
        #if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
            bitmapInfo |= kCGBitmapByteOrder32Little;
        #else
            bitmapInfo |= kCGBitmapByteOrder32Big;
        #endif
        
        CGImageRef image = CGImageCreate(
            self.bufferWidth,
            self.bufferHeight,
            8,
            32,
            self.bufferWidth * 4,
            colorSpace,
            bitmapInfo,
            provider,
            NULL,
            NO,
            kCGRenderingIntentDefault
        );
        
        if (image) {
            CGRect viewRect = [self bounds];
            CGContextDrawImage(context, viewRect, image);
            CGImageRelease(image);
        }
        
        if (provider) {
            CGDataProviderRelease(provider);
        }
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
    
    NSView* view = (__bridge NSView*)canvas;
    NSRect bounds = [view bounds];
    *width = (unsigned int)bounds.size.width;
    *height = (unsigned int)bounds.size.height;
}

