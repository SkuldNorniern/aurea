// Open, save and folder panels.

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
