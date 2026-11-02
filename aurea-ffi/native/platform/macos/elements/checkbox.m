#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_checkbox(const char* label) {
    NSButton* checkbox = [[NSButton alloc] init];
    [checkbox setButtonType:NSButtonTypeSwitch];
    [checkbox setTitle:label ? ng_macos_to_nsstring(label) : @""];
    [checkbox setState:NSControlStateValueOff];
    [checkbox setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)checkbox;
}

/* AppKit groups radio buttons by superview and action, so each has the action from the start;
   ng_macos_element_set_change_id keeps it and only changes the target. */
@interface NGRadioTarget : NSObject
- (void)changed:(id)sender;
@end

@implementation NGRadioTarget
- (void)changed:(id)sender {
    (void)sender;
}
@end

NGHandle ng_macos_create_radio(const char* label, NGHandle previous) {
    (void)previous;
    static NGRadioTarget* target = nil;
    if (!target) target = [[NGRadioTarget alloc] init];
    NSButton* radio = [[NSButton alloc] init];
    [radio setButtonType:NSButtonTypeRadio];
    [radio setTitle:label ? ng_macos_to_nsstring(label) : @""];
    [radio setState:NSControlStateValueOff];
    [radio setTarget:target];
    [radio setAction:@selector(changed:)];
    [radio setTranslatesAutoresizingMaskIntoConstraints:NO];
    return (__bridge_retained void*)radio;
}

int ng_macos_checkbox_set_checked(NGHandle checkbox, int checked) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    NSButton* nsCheckbox = (__bridge NSButton*)checkbox;
    [nsCheckbox setState:checked ? NSControlStateValueOn : NSControlStateValueOff];
    return NG_SUCCESS;
}

int ng_macos_checkbox_get_checked(NGHandle checkbox) {
    if (!checkbox) return 0;
    
    NSButton* nsCheckbox = (__bridge NSButton*)checkbox;
    return [nsCheckbox state] == NSControlStateValueOn ? 1 : 0;
}

int ng_macos_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    if (!checkbox) return NG_ERROR_INVALID_HANDLE;
    
    NSButton* nsCheckbox = (__bridge NSButton*)checkbox;
    [nsCheckbox setEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

void ng_macos_checkbox_invalidate(NGHandle checkbox) {
    if (!checkbox) return;
    NSButton* nsCheckbox = (__bridge NSButton*)checkbox;
    [nsCheckbox setNeedsDisplay:YES];
}

