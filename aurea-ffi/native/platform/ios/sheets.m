#import "host.h"
#import "present.h"
#import <QuickLook/QuickLook.h>

/* Alerts, the share sheet, Quick Look and handing things to other apps. None of them blocks: the app keeps drawing, and `done` hears the answer
   later on the main thread. */

/* UIKit's own word for a button, in the user's language, when it has one. */
static NSString* ng_ios_label(NSString* key) {
    NSBundle* uikit = [NSBundle bundleForClass:[UIApplication class]];
    return [uikit localizedStringForKey:key value:key table:nil];
}

/* The answers, as MessageChoice codes. */
enum { NG_IOS_OK = 0, NG_IOS_CANCEL = 1, NG_IOS_YES = 2, NG_IOS_NO = 3, NG_IOS_SAVE = 4, NG_IOS_DISCARD = 5 };

int ng_ios_alert(const char* title, const char* message, int buttons, NGIosAnswered done, void* ctx) {
    UIViewController* presenter = ng_ios_presenter();
    if (!presenter) return 0;
    UIAlertController* alert =
        [UIAlertController alertControllerWithTitle:title ? @(title) : nil
                                            message:message && *message ? @(message) : nil
                                     preferredStyle:UIAlertControllerStyleAlert];
    void (^add)(NSString*, UIAlertActionStyle, int) = ^(NSString* label, UIAlertActionStyle style, int answer) {
        [alert addAction:[UIAlertAction actionWithTitle:ng_ios_label(label)
                                                  style:style
                                                handler:^(UIAlertAction* action) {
                                                    (void)action;
                                                    if (done) done(ctx, answer);
                                                    ng_ios_wake();
                                                }]];
    };
    switch (buttons) {
        case 1:
            add(@"Cancel", UIAlertActionStyleCancel, NG_IOS_CANCEL);
            add(@"OK", UIAlertActionStyleDefault, NG_IOS_OK);
            break;
        case 2:
            add(@"No", UIAlertActionStyleCancel, NG_IOS_NO);
            add(@"Yes", UIAlertActionStyleDefault, NG_IOS_YES);
            break;
        case 3:
            add(@"Save", UIAlertActionStyleDefault, NG_IOS_SAVE);
            add(@"Don't Save", UIAlertActionStyleDestructive, NG_IOS_DISCARD);
            add(@"Cancel", UIAlertActionStyleCancel, NG_IOS_CANCEL);
            break;
        default:
            add(@"OK", UIAlertActionStyleDefault, NG_IOS_OK);
            break;
    }
    [presenter presentViewController:alert animated:YES completion:nil];
    return 1;
}

int ng_ios_share_files(const char* paths, float x, float y, float width, float height, NGIosAnswered done,
                       void* ctx) {
    UIViewController* presenter = ng_ios_presenter();
    if (!presenter) return 0;
    NSMutableArray<NSURL*>* urls = [NSMutableArray array];
    for (const char* at = paths; at && *at; at += strlen(at) + 1) {
        [urls addObject:[NSURL fileURLWithFileSystemRepresentation:at isDirectory:NO relativeToURL:nil]];
    }
    if (urls.count == 0) return 0;
    UIActivityViewController* share = [[UIActivityViewController alloc] initWithActivityItems:urls
                                                                       applicationActivities:nil];
    share.completionWithItemsHandler = ^(UIActivityType type, BOOL completed, NSArray* items, NSError* error) {
        (void)type;
        (void)items;
        (void)error;
        if (done) done(ctx, completed ? 1 : 0);
        ng_ios_wake();
    };
    /* An iPad shows the sheet as a popover, pointing at what was tapped. */
    UIPopoverPresentationController* popover = share.popoverPresentationController;
    if (popover) {
        UIView* view = presenter.view;
        UIEdgeInsets safe = view.safeAreaInsets;
        popover.sourceView = view;
        popover.sourceRect = width > 0 && height > 0
                                 ? CGRectMake(x + safe.left, y + safe.top, width, height)
                                 : CGRectMake(CGRectGetMidX(view.bounds), CGRectGetMidY(view.bounds), 1, 1);
        if (!(width > 0 && height > 0)) popover.permittedArrowDirections = 0;
    }
    [presenter presentViewController:share animated:YES completion:nil];
    return 1;
}

/* Quick Look shows one file; the data source lives as long as the preview. */
@interface AureaPreview : NSObject <QLPreviewControllerDataSource, QLPreviewControllerDelegate>
@property (strong) NSURL* url;
@end

static AureaPreview* g_preview;

@implementation AureaPreview
- (NSInteger)numberOfPreviewItemsInPreviewController:(QLPreviewController*)controller {
    (void)controller;
    return 1;
}
- (id<QLPreviewItem>)previewController:(QLPreviewController*)controller previewItemAtIndex:(NSInteger)index {
    (void)controller;
    (void)index;
    return _url;
}
- (void)previewControllerDidDismiss:(QLPreviewController*)controller {
    (void)controller;
    if (g_preview == self) g_preview = nil;
    ng_ios_wake();
}
@end

int ng_ios_preview_path(const char* path) {
    UIViewController* presenter = ng_ios_presenter();
    if (!presenter || !path) return 0;
    NSURL* url = [NSURL fileURLWithFileSystemRepresentation:path isDirectory:NO relativeToURL:nil];
    if (![QLPreviewController canPreviewItem:url]) return 0;
    AureaPreview* source = [[AureaPreview alloc] init];
    source.url = url;
    g_preview = source;
    QLPreviewController* preview = [[QLPreviewController alloc] init];
    preview.dataSource = source;
    preview.delegate = source;
    [presenter presentViewController:preview animated:YES completion:nil];
    return 1;
}

/* The Files app opens at a folder of the app's own Documents with this scheme. */
int ng_ios_reveal_path(const char* path) {
    if (!path) return 0;
    NSString* folder = [@(path) stringByDeletingLastPathComponent];
    NSURLComponents* parts = [[NSURLComponents alloc] init];
    parts.scheme = @"shareddocuments";
    /* An empty host keeps the `//`: shareddocuments:///path. */
    parts.host = @"";
    parts.path = folder;
    NSURL* url = parts.URL;
    if (!url || ![UIApplication.sharedApplication canOpenURL:url]) return 0;
    [UIApplication.sharedApplication openURL:url options:@{} completionHandler:nil];
    return 1;
}

int ng_ios_open_url(const char* text) {
    NSURL* url = text ? [NSURL URLWithString:@(text)] : nil;
    if (!url || !url.scheme) return 0;
    [UIApplication.sharedApplication openURL:url options:@{} completionHandler:nil];
    return 1;
}

