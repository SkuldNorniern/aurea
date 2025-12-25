#import "elements.h"
#import "utils.h"
#import "../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_button(const char* title) {
    @autoreleasepool {
        if (!title) {
            NSLog(@"Error: Invalid button title");
            return NULL;
        }
        
        NSButton* button = [[NSButton alloc] init];
        [button setTitle:ng_macos_to_nsstring(title)];
        [button setBezelStyle:NSBezelStyleRounded];
        [button setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        // Set reasonable default size constraints
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
    [stack setSpacing:8.0];
    [stack setAlignment:NSLayoutAttributeCenterY];
    
    return (__bridge_retained void*)stack;
}

int ng_macos_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;
    
    NSStackView* stack = (__bridge NSStackView*)box;
    NSView* view = (__bridge NSView*)element;
    
    [stack addArrangedSubview:view];
    
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
    
    NSScrollView* scrollView = (__bridge NSScrollView*)text_handle;
    NSTextView* textView = (NSTextView*)[scrollView documentView];
    
    [textView setString:ng_macos_to_nsstring(content)];
    
    return NG_SUCCESS;
}

char* ng_macos_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    NSScrollView* scrollView = (__bridge NSScrollView*)text_handle;
    NSTextView* textView = (NSTextView*)[scrollView documentView];
    
    const char* utf8String = [[textView string] UTF8String];
    if (!utf8String) return NULL;
    
    // Create a copy of the string that can be freed by the caller
    return strdup(utf8String);
}

void ng_macos_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_macos_create_canvas(int width, int height) {
    @autoreleasepool {
        // Create a custom view for rendering
        // This will be extended to support Metal/OpenGL surfaces
        NSView* canvasView = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
        [canvasView setWantsLayer:YES];
        [canvasView setLayer:[CALayer layer]];
        [canvasView.layer setBackgroundColor:[[NSColor whiteColor] CGColor]];
        
        return (__bridge_retained void*)canvasView;
    }
}

void ng_macos_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    
    NSView* view = (__bridge NSView*)canvas;
    [view setNeedsDisplay:YES];
} 