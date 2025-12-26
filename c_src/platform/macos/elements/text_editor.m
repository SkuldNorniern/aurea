#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

extern void ng_invoke_text_callback(unsigned int id, const char* content);

@interface TextEditorDelegate : NSObject <NSTextViewDelegate>
@property (nonatomic, assign) unsigned int textId;
@end

@implementation TextEditorDelegate
- (void)textDidChange:(NSNotification*)notification {
    NSTextView* textView = (NSTextView*)[notification object];
    NSString* content = [textView string];
    if (content && self.textId != 0) {
        const char* utf8 = [content UTF8String];
        if (utf8) {
            ng_invoke_text_callback(self.textId, utf8);
        }
    }
}
@end

NGHandle ng_macos_create_text_editor(unsigned int id) {
    @autoreleasepool {
        NSScrollView* scrollView = [[NSScrollView alloc] init];
        NSTextView* textView = [[NSTextView alloc] init];
        
        [scrollView setHasVerticalScroller:YES];
        [scrollView setHasHorizontalScroller:YES];
        [scrollView setAutohidesScrollers:YES];
        [scrollView setBorderType:NSBezelBorder];
        [scrollView setTranslatesAutoresizingMaskIntoConstraints:NO];
        
        [textView setEditable:YES];
        [textView setSelectable:YES];
        [textView setAllowsUndo:YES];
        [textView setAutomaticQuoteSubstitutionEnabled:NO];
        [textView setAutomaticDashSubstitutionEnabled:NO];
        [textView setEnabledTextCheckingTypes:0];
        [textView setRichText:NO];
        [textView setFont:[NSFont fontWithName:@"Menlo" size:12.0]];
        [textView setTextContainerInset:NSMakeSize(5, 5)];
        
        NSTextContainer *container = [textView textContainer];
        [container setWidthTracksTextView:YES];
        [container setHeightTracksTextView:NO];
        
        [textView setHorizontallyResizable:YES];
        [textView setVerticallyResizable:YES];
        [textView setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
        
        if (id != 0) {
            TextEditorDelegate* delegate = [[TextEditorDelegate alloc] init];
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

