
#import "dialog.h"
#import "utils.h"
#import "common/errors.h"
#import <Cocoa/Cocoa.h>
#include <stdlib.h>
#include <string.h>

/* Each filter is a name and the extensions it allows; nil allows every file. */
@interface NGDialogFilter : NSObject
@property (copy) NSString* name;
@property (copy) NSArray<NSString*>* extensions;
@end

@implementation NGDialogFilter
@end

static NSArray<NGDialogFilter*>* filters_of(const char* filters) {
    NSMutableArray<NGDialogFilter*>* out = [NSMutableArray array];
    if (!filters || !*filters) return out;
    for (NSString* line in [ng_macos_to_nsstring(filters) componentsSeparatedByString:@"\n"]) {
        NSRange bar = [line rangeOfString:@"|"];
        if (bar.location == NSNotFound) continue;
        NGDialogFilter* filter = [[NGDialogFilter alloc] init];
        filter.name = [line substringToIndex:bar.location];
        NSMutableArray<NSString*>* exts = [NSMutableArray array];
        BOOL any = NO;
        for (NSString* ext in [[line substringFromIndex:bar.location + 1] componentsSeparatedByString:@";"]) {
            if ([ext isEqualToString:@"*"]) any = YES;
            else if (ext.length > 0) [exts addObject:ext];
        }
        filter.extensions = (any || exts.count == 0) ? nil : exts;
        [out addObject:filter];
    }
    return out;
}

static void allow_filter(NSSavePanel* panel, NGDialogFilter* filter) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    [panel setAllowedFileTypes:filter.extensions];
#pragma clang diagnostic pop
}

@interface NGFilterPicker : NSObject
@property (weak) NSSavePanel* panel;
@property (copy) NSArray<NGDialogFilter*>* filters;
- (void)picked:(NSPopUpButton*)sender;
@end

@implementation NGFilterPicker
- (void)picked:(NSPopUpButton*)sender {
    NSInteger index = [sender indexOfSelectedItem];
    if (index >= 0 && index < (NSInteger)self.filters.count) allow_filter(self.panel, self.filters[index]);
}
@end

static NSView* filter_picker_view(NGFilterPicker* picker) {
    NSTextField* label = [NSTextField labelWithString:@"Format:"];
    NSPopUpButton* popup = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    for (NGDialogFilter* filter in picker.filters) {
        [[popup menu] addItemWithTitle:filter.name action:nil keyEquivalent:@""];
    }
    popup.target = picker;
    popup.action = @selector(picked:);
    [label sizeToFit];
    [popup sizeToFit];
    CGFloat height = MAX(label.frame.size.height, popup.frame.size.height) + 16;
    CGFloat width = label.frame.size.width + popup.frame.size.width + 48;
    NSView* row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
    [label setFrameOrigin:NSMakePoint(16, (height - label.frame.size.height) / 2)];
    [popup setFrameOrigin:NSMakePoint(NSMaxX(label.frame) + 8, (height - popup.frame.size.height) / 2)];
    [row addSubview:label];
    [row addSubview:popup];
    return row;
}

static char* joined(NSArray<NSURL*>* urls) {
    NSMutableData* list = [NSMutableData data];
    for (NSURL* url in urls) {
        const char* path = url.fileURL ? url.fileSystemRepresentation : NULL;
        if (path && *path) [list appendBytes:path length:strlen(path) + 1];
    }
    if (list.length == 0) return NULL;
    [list appendBytes:"" length:1];
    char* out = (char*)malloc(list.length);
    if (out) memcpy(out, list.bytes, list.length);
    return out;
}

char* ng_macos_file_dialog(NGHandle window, int kind, const char* title, const char* filters,
                           const char* default_name, const char* directory, int multiple) {
    (void)window;
    @autoreleasepool {
        NSSavePanel* panel;
        if (kind == NG_DIALOG_SAVE) {
            panel = [NSSavePanel savePanel];
            if (default_name && *default_name) {
                [panel setNameFieldStringValue:ng_macos_to_nsstring(default_name)];
            }
        } else {
            NSOpenPanel* open = [NSOpenPanel openPanel];
            BOOL folder = kind == NG_DIALOG_FOLDER;
            [open setCanChooseDirectories:folder];
            [open setCanChooseFiles:!folder];
            [open setAllowsMultipleSelection:(multiple && !folder) ? YES : NO];
            panel = open;
        }
        if (title && *title) {
            [panel setMessage:ng_macos_to_nsstring(title)];
            [panel setTitle:ng_macos_to_nsstring(title)];
        }
        NSArray<NGDialogFilter*>* list = kind == NG_DIALOG_FOLDER ? @[] : filters_of(filters);
        __attribute__((objc_precise_lifetime)) NGFilterPicker* picker = nil;
        if (list.count > 0) allow_filter(panel, list[0]);
        if (list.count > 1) {
            picker = [[NGFilterPicker alloc] init];
            picker.panel = panel;
            picker.filters = list;
            panel.accessoryView = filter_picker_view(picker);
            if ([panel isKindOfClass:[NSOpenPanel class]]) {
                ((NSOpenPanel*)panel).accessoryViewDisclosed = YES;
            }
        }
        if (directory && *directory) {
            [panel setDirectoryURL:[NSURL fileURLWithPath:ng_macos_to_nsstring(directory) isDirectory:YES]];
        }
        if ([panel runModal] != NSModalResponseOK) return NULL;
        if (kind == NG_DIALOG_SAVE) return panel.URL ? joined(@[panel.URL]) : NULL;
        return joined([(NSOpenPanel*)panel URLs]);
    }
}

typedef struct {
    const char* title;
    int choice;
} Button;

static const Button ok_buttons[] = {{"OK", NG_CHOICE_OK}};
static const Button ok_cancel_buttons[] = {{"OK", NG_CHOICE_OK}, {"Cancel", NG_CHOICE_CANCEL}};
static const Button yes_no_buttons[] = {{"Yes", NG_CHOICE_YES}, {"No", NG_CHOICE_NO}};
static const Button save_discard_buttons[] = {
    {"Save", NG_CHOICE_SAVE}, {"Cancel", NG_CHOICE_CANCEL}, {"Don't Save", NG_CHOICE_DISCARD}};

static const Button* buttons_of(int buttons, NSInteger* count) {
    switch (buttons) {
    case NG_MESSAGE_BUTTONS_OK_CANCEL:
        *count = 2;
        return ok_cancel_buttons;
    case NG_MESSAGE_BUTTONS_YES_NO:
        *count = 2;
        return yes_no_buttons;
    case NG_MESSAGE_BUTTONS_SAVE_DISCARD_CANCEL:
        *count = 3;
        return save_discard_buttons;
    default:
        *count = 1;
        return ok_buttons;
    }
}

static int closed_choice(int buttons) {
    if (buttons == NG_MESSAGE_BUTTONS_YES_NO) return NG_CHOICE_NO;
    if (buttons == NG_MESSAGE_BUTTONS_OK) return NG_CHOICE_OK;
    return NG_CHOICE_CANCEL;
}

int ng_macos_message_box(NGHandle window, int kind, const char* title, const char* text, int buttons) {
    (void)window;
    @autoreleasepool {
        NSAlert* alert = [[NSAlert alloc] init];
        alert.alertStyle = kind == NG_MESSAGE_ERROR ? NSAlertStyleCritical
            : kind == NG_MESSAGE_WARNING ? NSAlertStyleWarning
            : NSAlertStyleInformational;
        if (title && *title) alert.messageText = ng_macos_to_nsstring(title);
        if (text && *text) alert.informativeText = ng_macos_to_nsstring(text);
        NSInteger count = 0;
        const Button* list = buttons_of(buttons, &count);
        for (NSInteger i = 0; i < count; i++) {
            [alert addButtonWithTitle:ng_macos_to_nsstring(list[i].title)];
        }
        if (buttons == NG_MESSAGE_BUTTONS_YES_NO) [alert.buttons[1] setKeyEquivalent:@"\e"];
        NSInteger index = [alert runModal] - NSAlertFirstButtonReturn;
        if (index < 0 || index >= count) return closed_choice(buttons);
        return list[index].choice;
    }
}

static unsigned int channel_of(CGFloat v) {
    CGFloat scaled = v * 255.0 + 0.5;
    if (scaled < 0) return 0;
    if (scaled > 255) return 255;
    return (unsigned int)scaled;
}

@interface AureaColorPanelButtons : NSObject
- (void)pick:(id)sender;
- (void)cancel:(id)sender;
@end

@implementation AureaColorPanelButtons
- (void)pick:(id)sender {
    (void)sender;
    [NSApp stopModalWithCode:NSModalResponseOK];
}
- (void)cancel:(id)sender {
    (void)sender;
    [NSApp stopModalWithCode:NSModalResponseCancel];
}
@end

static NSView* color_panel_buttons(AureaColorPanelButtons* target) {
    NSView* row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 220, 44)];
    NSButton* cancel = [NSButton buttonWithTitle:@"Cancel" target:target action:@selector(cancel:)];
    cancel.keyEquivalent = @"\e";
    cancel.frame = NSMakeRect(20, 8, 88, 28);
    NSButton* ok = [NSButton buttonWithTitle:@"OK" target:target action:@selector(pick:)];
    ok.keyEquivalent = @"\r";
    ok.frame = NSMakeRect(112, 8, 88, 28);
    [row addSubview:cancel];
    [row addSubview:ok];
    return row;
}

int ng_macos_color_dialog(NGHandle window, const char* title, unsigned int argb_in, int alpha, unsigned int* argb_out) {
    (void)window;
    @autoreleasepool {
        unsigned int a_in = (argb_in >> 24) & 0xFF;
        NSColor* start = [NSColor colorWithSRGBRed:((argb_in >> 16) & 0xFF) / 255.0
                                             green:((argb_in >> 8) & 0xFF) / 255.0
                                              blue:(argb_in & 0xFF) / 255.0
                                             alpha:alpha ? a_in / 255.0 : 1.0];
        NSColorPanel* panel = [NSColorPanel sharedColorPanel];
        if (title && *title) panel.title = ng_macos_to_nsstring(title);
        panel.showsAlpha = alpha ? YES : NO;
        [panel setColor:start];

        AureaColorPanelButtons* target = [[AureaColorPanelButtons alloc] init];
        NSView* previous = panel.accessoryView;
        panel.accessoryView = color_panel_buttons(target);

        id observer = [[NSNotificationCenter defaultCenter]
            addObserverForName:NSWindowWillCloseNotification
                        object:panel
                         queue:nil
                    usingBlock:^(NSNotification* note) {
                        (void)note;
                        [NSApp stopModalWithCode:NSModalResponseCancel];
                    }];
        [panel makeKeyAndOrderFront:nil];
        NSModalResponse answer = [NSApp runModalForWindow:panel];
        [[NSNotificationCenter defaultCenter] removeObserver:observer];
        panel.accessoryView = previous;
        [panel orderOut:nil];
        if (answer != NSModalResponseOK) return 0;

        NSColor* picked = [panel.color colorUsingColorSpace:[NSColorSpace sRGBColorSpace]];
        if (!picked) return NG_ERROR_PLATFORM_SPECIFIC;
        CGFloat r = 0, g = 0, b = 0, a = 1;
        [picked getRed:&r green:&g blue:&b alpha:&a];
        unsigned int a_out = alpha ? channel_of(a) : a_in;
        *argb_out = (a_out << 24) | (channel_of(r) << 16) | (channel_of(g) << 8) | channel_of(b);
        return 1;
    }
}
