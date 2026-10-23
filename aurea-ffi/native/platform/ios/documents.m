#import "host.h"
#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

/* The system document picker, for files to open and files to save elsewhere. It is not modal: the
   app keeps running, and `done` hears the paths later on the main thread. */

/* The view controller on top, for the picker to present over. */
static UIViewController* ng_ios_presenter(void) {
    for (UIScene* scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:[UIWindowScene class]]) continue;
        for (UIWindow* window in ((UIWindowScene*)scene).windows) {
            if (!window.isKeyWindow) continue;
            UIViewController* top = window.rootViewController;
            while (top.presentedViewController) top = top.presentedViewController;
            return top;
        }
    }
    return nil;
}

/* `urls` as NUL-terminated paths, ended by an empty one. */
static NSData* ng_ios_path_list(NSArray<NSURL*>* urls) {
    NSMutableData* list = [NSMutableData data];
    for (NSURL* url in urls) {
        const char* path = url.fileSystemRepresentation;
        if (path) [list appendBytes:path length:strlen(path) + 1];
    }
    [list appendBytes:"" length:1];
    return list;
}

@interface AureaDocumentPicker : NSObject <UIDocumentPickerDelegate>
@property (assign) NGIosPicked done;
@property (assign) void* ctx;
@end

/* Pickers waiting for the user: the controller holds its delegate weakly. */
static NSMutableSet<AureaDocumentPicker*>* g_pickers;

@implementation AureaDocumentPicker

- (void)finish:(NSArray<NSURL*>*)urls {
    NSData* list = ng_ios_path_list(urls);
    if (_done) _done(_ctx, (const char*)list.bytes);
    _done = NULL;
    [g_pickers removeObject:self];
    ng_ios_wake();
}

- (void)documentPicker:(UIDocumentPickerViewController*)controller didPickDocumentsAtURLs:(NSArray<NSURL*>*)urls {
    (void)controller;
    [self finish:urls];
}

- (void)documentPickerWasCancelled:(UIDocumentPickerViewController*)controller {
    (void)controller;
    [self finish:@[]];
}

@end

static int ng_ios_present_picker(UIDocumentPickerViewController* picker, NGIosPicked done, void* ctx) {
    UIViewController* presenter = ng_ios_presenter();
    if (!picker || !presenter) return 0;
    if (!g_pickers) g_pickers = [NSMutableSet set];
    AureaDocumentPicker* delegate = [[AureaDocumentPicker alloc] init];
    delegate.done = done;
    delegate.ctx = ctx;
    [g_pickers addObject:delegate];
    picker.delegate = delegate;
    [presenter presentViewController:picker animated:YES completion:nil];
    return 1;
}

int ng_ios_pick_files(const char* extensions, int multiple, NGIosPicked done, void* ctx) {
    UIDocumentPickerViewController* picker = nil;
    if (@available(iOS 14.0, *)) {
        NSMutableArray<UTType*>* types = [NSMutableArray array];
        for (const char* at = extensions; at && *at; at += strlen(at) + 1) {
            UTType* type = [UTType typeWithFilenameExtension:@(at)];
            if (type) [types addObject:type];
        }
        if (types.count == 0) [types addObject:UTTypeItem];
        picker = [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:types asCopy:YES];
    } else {
        picker = [[UIDocumentPickerViewController alloc] initWithDocumentTypes:@[ @"public.item" ]
                                                                        inMode:UIDocumentPickerModeImport];
    }
    picker.allowsMultipleSelection = multiple != 0;
    return ng_ios_present_picker(picker, done, ctx);
}

int ng_ios_export_files(const char* paths, NGIosPicked done, void* ctx) {
    NSMutableArray<NSURL*>* urls = [NSMutableArray array];
    for (const char* at = paths; at && *at; at += strlen(at) + 1) {
        [urls addObject:[NSURL fileURLWithFileSystemRepresentation:at isDirectory:NO relativeToURL:nil]];
    }
    if (urls.count == 0) return 0;
    UIDocumentPickerViewController* picker = nil;
    if (@available(iOS 14.0, *)) {
        picker = [[UIDocumentPickerViewController alloc] initForExportingURLs:urls asCopy:YES];
    } else {
        picker = [[UIDocumentPickerViewController alloc] initWithURLs:urls inMode:UIDocumentPickerModeExportToService];
    }
    return ng_ios_present_picker(picker, done, ctx);
}
