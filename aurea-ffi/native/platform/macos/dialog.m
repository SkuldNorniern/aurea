// Open, save and folder panels, and message boxes.

#import "dialog.h"
#import "utils.h"
#import <Cocoa/Cocoa.h>
#include <stdlib.h>
#include <string.h>

/* Every extension of "Images|png;jpg\nAll|*", or nil when one is "*". */
static NSArray<NSString*>* extensions_of(const char* filters) {
    if (!filters || !*filters) return nil;
    NSMutableArray<NSString*>* out = [NSMutableArray array];
    NSString* all = ng_macos_to_nsstring(filters);
    for (NSString* line in [all componentsSeparatedByString:@"\n"]) {
        NSRange bar = [line rangeOfString:@"|"];
        if (bar.location == NSNotFound) continue;
        NSString* exts = [line substringFromIndex:bar.location + 1];
        for (NSString* ext in [exts componentsSeparatedByString:@";"]) {
            if ([ext isEqualToString:@"*"]) return nil;
            if (ext.length > 0) [out addObject:ext];
        }
    }
    return out.count > 0 ? out : nil;
}

static char* joined(NSArray<NSURL*>* urls) {
    NSMutableArray<NSString*>* paths = [NSMutableArray array];
    for (NSURL* url in urls) {
        if (url.path) [paths addObject:url.path];
    }
    if (paths.count == 0) return NULL;
    return strdup([[paths componentsJoinedByString:@"\n"] UTF8String]);
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
        NSArray<NSString*>* exts = kind == NG_DIALOG_FOLDER ? nil : extensions_of(filters);
        if (exts) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            [panel setAllowedFileTypes:exts];
#pragma clang diagnostic pop
        }
        if (directory && *directory) {
            [panel setDirectoryURL:[NSURL fileURLWithPath:ng_macos_to_nsstring(directory) isDirectory:YES]];
        }
        /* Modal to the app, like the other platforms; the run loop keeps
           turning, so timers and display links still fire. */
        if ([panel runModal] != NSModalResponseOK) return NULL;
        if (kind == NG_DIALOG_SAVE) return panel.URL ? joined(@[panel.URL]) : NULL;
        return joined([(NSOpenPanel*)panel URLs]);
    }
}

typedef struct {
    const char* title;
    int choice;
} Button;

/* Each set in macOS order, with the answer each button gives. */
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

/* The answer closing the box gives when no button answers it. */
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
        /* Escape answers No, as Cancel does on the other sets. */
        if (buttons == NG_MESSAGE_BUTTONS_YES_NO) [alert.buttons[1] setKeyEquivalent:@"\e"];
        /* Modal to the app, like the other platforms; the run loop keeps
           turning, so timers and display links still fire. */
        NSInteger index = [alert runModal] - NSAlertFirstButtonReturn;
        if (index < 0 || index >= count) return closed_choice(buttons);
        return list[index].choice;
    }
}
