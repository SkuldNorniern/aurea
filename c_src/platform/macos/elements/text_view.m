#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

extern void ng_invoke_textview_callback(unsigned int id, const char* content);

@interface TextViewDelegate : NSObject <NSTextViewDelegate>
@property (nonatomic, assign) unsigned int textId;
@end

@implementation TextViewDelegate
- (void)textDidChange:(NSNotification*)notification {
    NSTextView* textView = (NSTextView*)[notification object];
    NSString* content = [textView string];
    if (content && self.textId != 0) {
        const char* utf8 = [content UTF8String];
        if (utf8) {
            ng_invoke_textview_callback(self.textId, utf8);
        }
    }
}
@end

NGHandle ng_macos_create_text_view(int is_editable, unsigned int id) {
    @autoreleasepool {
        NSScrollView* scrollView = [[NSScrollView alloc] init];
        NSTextView* textView = [[NSTextView alloc] init];
        
        [scrollView setHasVerticalScroller:YES];
        [scrollView setHasHorizontalScroller:YES];
        [scrollView setAutohidesScrollers:YES];
        [scrollView setBorderType:NSBezelBorder];
        [scrollView setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        [textView setEditable:is_editable ? YES : NO];
        [textView setSelectable:YES];
        [textView setRichText:NO];
        [textView setFont:[NSFont fontWithName:@"Menlo" size:12.0]];
        [textView setTextContainerInset:NSMakeSize(5, 5)];
        
        NSTextContainer *container = [textView textContainer];
        [container setWidthTracksTextView:YES];
        [container setHeightTracksTextView:NO];
        
        [textView setHorizontallyResizable:YES];
        [textView setVerticallyResizable:YES];
        [textView setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
        
        if (!is_editable) {
            [textView setBackgroundColor:[NSColor blackColor]];
            [textView setTextColor:[NSColor greenColor]];
        }
        
        if (id != 0 && is_editable) {
            TextViewDelegate* delegate = [[TextViewDelegate alloc] init];
            delegate.textId = id;
            [textView setDelegate:delegate];
            objc_setAssociatedObject(textView, "delegate", delegate, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        }
        
        [scrollView setDocumentView:textView];
        [scrollView setHasVerticalRuler:NO];
        [scrollView setHasHorizontalRuler:NO];
        
        return (__bridge_retained void*)scrollView;
    }
}

