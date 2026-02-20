#import "../elements.h"
#import "../utils.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

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

void ng_macos_button_invalidate(NGHandle button) {
    if (!button) return;
    NSButton* btn = (__bridge NSButton*)button;
    [btn setNeedsDisplay:YES];
}
