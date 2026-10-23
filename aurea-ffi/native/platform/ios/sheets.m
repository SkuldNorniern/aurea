#import "host.h"
#import "present.h"

/* Alerts and the share sheet. Neither blocks: the app keeps drawing, and `done` hears the answer
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
