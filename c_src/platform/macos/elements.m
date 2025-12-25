#import "elements.h"
#import "utils.h"
#import "../../common/errors.h"
#import <Cocoa/Cocoa.h>

extern void ng_invoke_button_callback(unsigned int id);

@interface ButtonTarget : NSObject
- (void)buttonClicked:(id)sender;
@end

@implementation ButtonTarget
- (void)buttonClicked:(id)sender {
    NSButton* button = (NSButton*)sender;
    unsigned int id = (unsigned int)[button tag];
    ng_invoke_button_callback(id);
}
@end

static ButtonTarget* buttonTarget = nil;

NGHandle ng_macos_create_button(const char* title, unsigned int id) {
    @autoreleasepool {
        if (!title) {
            NSLog(@"Error: Invalid button title");
            return NULL;
        }
        
        if (!buttonTarget) {
            buttonTarget = [[ButtonTarget alloc] init];
        }
        
        NSButton* button = [[NSButton alloc] init];
        [button setTitle:ng_macos_to_nsstring(title)];
        [button setBezelStyle:NSBezelStyleRounded];
        [button setTranslatesAutoresizingMaskIntoConstraints:NO];
        [button setTarget:buttonTarget];
        [button setAction:@selector(buttonClicked:)];
        [button setTag:id];
        
        NSSize minSize = NSMakeSize(60, 24);
        [button setFrameSize:minSize];
        
        return (__bridge_retained void*)button;
    }
}

NGHandle ng_macos_create_label(const char* text) {
    if (!text) return NULL;
    
    NSTextField* label = [[NSTextField alloc] init];
    [label setStringValue:ng_macos_to_nsstring(text)];
    [label setBezeled:NO];
    [label setDrawsBackground:NO];
    [label setEditable:NO];
    [label setSelectable:NO];
    [label sizeToFit];
    
    return (__bridge_retained void*)label;
}

NGHandle ng_macos_create_box(int is_vertical) {
    NSStackView* stack = [[NSStackView alloc] init];
    [stack setOrientation:is_vertical ? NSUserInterfaceLayoutOrientationVertical 
                                    : NSUserInterfaceLayoutOrientationHorizontal];
    [stack setSpacing:4.0];
    [stack setAlignment:is_vertical ? NSLayoutAttributeLeading : NSLayoutAttributeCenterY];
    [stack setDistribution:NSStackViewDistributionFill];
    [stack setEdgeInsets:NSEdgeInsetsMake(4.0, 4.0, 4.0, 4.0)];
    
    return (__bridge_retained void*)stack;
}

int ng_macos_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSStackView* stack = (__bridge NSStackView*)box;
    NSView* view = (__bridge NSView*)element;
    
    // Configure view for proper layout
    [view setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    // Add to stack view
    [stack addArrangedSubview:view];
    
    // Set layout priorities based on view type
    // Canvas and text views should expand, buttons should hug content
    // Check for canvas by checking if it responds to renderBuffer selector
    if ([view respondsToSelector:@selector(renderBuffer)]) {
        // Canvas should expand to fill available space
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationVertical];
    } else if ([view isKindOfClass:[NSScrollView class]] || [view isKindOfClass:[NSTextField class]]) {
        // Text views and text fields should expand horizontally
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
    } else if ([view isKindOfClass:[NSButton class]]) {
        // Buttons should hug their content
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
    }
    
    return NG_SUCCESS;
}

NGHandle ng_macos_create_text_editor(void) {
    @autoreleasepool {
        NSScrollView* scrollView = [[NSScrollView alloc] init];
        NSTextView* textView = [[NSTextView alloc] init];
        
        [scrollView setHasVerticalScroller:YES];
        [scrollView setHasHorizontalScroller:YES];
        [scrollView setAutohidesScrollers:YES];
        [scrollView setBorderType:NSBezelBorder];
        [scrollView setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        // Make text view editable and selectable
        [textView setEditable:YES];
        [textView setSelectable:YES];
        
        // Enable input
        [textView setAllowsUndo:YES];
        [textView setAutomaticQuoteSubstitutionEnabled:NO];
        [textView setAutomaticDashSubstitutionEnabled:NO];
        [textView setEnabledTextCheckingTypes:0];
        [textView setRichText:NO];
        [textView setFont:[NSFont fontWithName:@"Menlo" size:12.0]];
        [textView setTextContainerInset:NSMakeSize(5, 5)];
        
        // Important: Set up the text container
        NSTextContainer *container = [textView textContainer];
        [container setWidthTracksTextView:YES];
        [container setHeightTracksTextView:NO];
        
        // Configure text view layout
        [textView setHorizontallyResizable:YES];
        [textView setVerticallyResizable:YES];
        [textView setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
        
        // Set up scroll view with text view
        [scrollView setDocumentView:textView];
        [scrollView setHasVerticalRuler:NO];
        [scrollView setHasHorizontalRuler:NO];
        
        return (__bridge_retained void*)scrollView;
    }
}

NGHandle ng_macos_create_text_view(int is_editable) {
    @autoreleasepool {
        NSScrollView* scrollView = [[NSScrollView alloc] init];
        NSTextView* textView = [[NSTextView alloc] init];
        
        [scrollView setHasVerticalScroller:YES];
        [scrollView setHasHorizontalScroller:YES];
        [scrollView setAutohidesScrollers:YES];
        [scrollView setBorderType:NSBezelBorder];
        [scrollView setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        // Configure text view
        [textView setEditable:is_editable ? YES : NO];
        [textView setSelectable:YES];
        [textView setRichText:NO];
        [textView setFont:[NSFont fontWithName:@"Menlo" size:12.0]];
        [textView setTextContainerInset:NSMakeSize(5, 5)];
        
        // Set up text container
        NSTextContainer *container = [textView textContainer];
        [container setWidthTracksTextView:YES];
        [container setHeightTracksTextView:NO];
        
        // Configure text view layout
        [textView setHorizontallyResizable:YES];
        [textView setVerticallyResizable:YES];
        [textView setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
        
        if (!is_editable) {
            [textView setBackgroundColor:[NSColor blackColor]];
            [textView setTextColor:[NSColor greenColor]];
        }
        
        // Set up scroll view with text view
        [scrollView setDocumentView:textView];
        [scrollView setHasVerticalRuler:NO];
        [scrollView setHasHorizontalRuler:NO];
        
        return (__bridge_retained void*)scrollView;
    }
}

int ng_macos_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    
    NSView* view = (__bridge NSView*)text_handle;
    NSString* nsContent = ng_macos_to_nsstring(content);
    
    // Handle NSTextField (for editable text like URL bar)
    if ([view isKindOfClass:[NSTextField class]]) {
        [(NSTextField*)view setStringValue:nsContent];
        return NG_SUCCESS;
    }
    
    // Handle NSScrollView with NSTextView
    if ([view isKindOfClass:[NSScrollView class]]) {
        NSScrollView* scrollView = (NSScrollView*)view;
        NSView* docView = [scrollView documentView];
        if ([docView isKindOfClass:[NSTextView class]]) {
            [(NSTextView*)docView setString:nsContent];
            return NG_SUCCESS;
        }
    }
    
    return NG_ERROR_INVALID_HANDLE;
}

char* ng_macos_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    NSView* view = (__bridge NSView*)text_handle;
    NSString* content = nil;
    
    // Handle NSTextField
    if ([view isKindOfClass:[NSTextField class]]) {
        content = [(NSTextField*)view stringValue];
    }
    // Handle NSScrollView with NSTextView
    else if ([view isKindOfClass:[NSScrollView class]]) {
        NSScrollView* scrollView = (NSScrollView*)view;
        NSView* docView = [scrollView documentView];
        if ([docView isKindOfClass:[NSTextView class]]) {
            content = [(NSTextView*)docView string];
        }
    }
    
    if (!content) return NULL;
    
    const char* utf8String = [content UTF8String];
    if (!utf8String) return NULL;
    
    // Create a copy of the string that can be freed by the caller
    return strdup(utf8String);
}

void ng_macos_free_text_content(char* content) {
    if (content) {
        free(content);
    }
} 

// Custom view class for canvas rendering
@interface AureaCanvasView : NSView
@property (nonatomic, assign) unsigned char* renderBuffer;
@property (nonatomic, assign) unsigned int bufferWidth;
@property (nonatomic, assign) unsigned int bufferHeight;
@end

@implementation AureaCanvasView
- (void)drawRect:(NSRect)dirtyRect {
    NSLog(@"AureaCanvasView::drawRect called, buffer=%p, size=%ux%u", 
          self.renderBuffer, self.bufferWidth, self.bufferHeight);
    
    if (self.renderBuffer && self.bufferWidth > 0 && self.bufferHeight > 0) {
        NSLog(@"Drawing buffer to view");
        CGContextRef context = [[NSGraphicsContext currentContext] CGContext];
        
        // Create a CGImage from the RGBA buffer
        // The buffer is in RGBA format, stored as u32 values (4 bytes per pixel)
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        
        // Create image directly from the buffer
        // Buffer format: u32 values with RGBA in big-endian format
        // kCGBitmapByteOrder32Big means R is in the most significant byte
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL,
            self.renderBuffer,
            self.bufferWidth * self.bufferHeight * 4, // total bytes
            NULL
        );
        
        // Determine byte order based on platform
        // macOS is little-endian, so we use kCGBitmapByteOrder32Little
        // The buffer stores u32 values with RGBA in native byte order
        CGBitmapInfo bitmapInfo = (CGBitmapInfo)kCGImageAlphaPremultipliedLast;
        
        // Check if we're on a little-endian system (most modern systems)
        #if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
            bitmapInfo |= kCGBitmapByteOrder32Little;
        #else
            bitmapInfo |= kCGBitmapByteOrder32Big;
        #endif
        
        CGImageRef image = CGImageCreate(
            self.bufferWidth,
            self.bufferHeight,
            8,  // bits per component
            32, // bits per pixel (RGBA = 4 bytes = 32 bits)
            self.bufferWidth * 4, // bytes per row
            colorSpace,
            bitmapInfo,
            provider,
            NULL,
            NO, // should interpolate
            kCGRenderingIntentDefault
        );
        
        if (image) {
            // Draw the image to fill the view
            CGRect viewRect = [self bounds];
            NSLog(@"Drawing image to rect: (%.1f, %.1f) %.1fx%.1f", 
                  viewRect.origin.x, viewRect.origin.y, viewRect.size.width, viewRect.size.height);
            CGContextDrawImage(context, viewRect, image);
            CGImageRelease(image);
            NSLog(@"Image drawn successfully");
        } else {
            NSLog(@"Failed to create CGImage from buffer");
        }
        
        if (provider) {
            CGDataProviderRelease(provider);
        }
        CGColorSpaceRelease(colorSpace);
    } else {
        // Fallback: draw white background
        NSLog(@"No buffer available, drawing white background");
        [[NSColor whiteColor] setFill];
        NSRectFill(dirtyRect);
    }
}

// ARC handles deallocation automatically
// Buffer is managed by Rust, don't free it here
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

void ng_macos_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size __attribute__((unused)), unsigned int width, unsigned int height) {
    NSLog(@"ng_macos_canvas_update_buffer: canvas=%p, buffer=%p, size=%u, %ux%u", 
          canvas, buffer, size, width, height);
    
    if (!canvas || !buffer) {
        NSLog(@"ng_macos_canvas_update_buffer: Invalid parameters");
        return;
    }
    
    AureaCanvasView* view = (__bridge AureaCanvasView*)canvas;
    if ([view isKindOfClass:[AureaCanvasView class]]) {
        // Store the buffer pointer (buffer is managed by Rust, we just reference it)
        view.renderBuffer = (unsigned char*)buffer;
        view.bufferWidth = width;
        view.bufferHeight = height;
        NSLog(@"Buffer updated, requesting display");
        [view setNeedsDisplay:YES];
    } else {
        NSLog(@"ng_macos_canvas_update_buffer: View is not AureaCanvasView");
    }
} 