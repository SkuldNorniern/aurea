#import "drop.h"
#import "host_view.h"
#include <string.h>

/* Files dropped on the app from others. */
@interface AureaDropDelegate : NSObject <UIDropInteractionDelegate>
@end

@implementation AureaDropDelegate

- (BOOL)dropInteraction:(UIDropInteraction*)interaction canHandleSession:(id<UIDropSession>)session {
    (void)interaction;
    return [session hasItemsConformingToTypeIdentifiers:@[ @"public.item" ]];
}

- (UIDropProposal*)dropInteraction:(UIDropInteraction*)interaction sessionDidUpdate:(id<UIDropSession>)session {
    (void)interaction; (void)session;
    return [[UIDropProposal alloc] initWithDropOperation:UIDropOperationCopy];
}

/* Each file only lasts its handler, so it is copied out first; the app hears them all at once. */
- (void)dropInteraction:(UIDropInteraction*)interaction performDrop:(id<UIDropSession>)session {
    CGPoint at = [session locationInView:interaction.view];
    dispatch_group_t waiting = dispatch_group_create();
    NSMutableArray<NSString*>* paths = [NSMutableArray array];
    NSString* folder = [NSTemporaryDirectory() stringByAppendingPathComponent:NSUUID.UUID.UUIDString];
    [NSFileManager.defaultManager createDirectoryAtPath:folder withIntermediateDirectories:YES attributes:nil error:nil];
    for (UIDragItem* item in session.items) {
        NSItemProvider* provider = item.itemProvider;
        NSString* type = provider.registeredTypeIdentifiers.firstObject;
        if (!type) continue;
        dispatch_group_enter(waiting);
        [provider loadFileRepresentationForTypeIdentifier:type completionHandler:^(NSURL* url, NSError* error) {
            (void)error;
            NSString* name = provider.suggestedName ?: url.lastPathComponent;
            if (url.pathExtension.length && !name.pathExtension.length) name = [name stringByAppendingPathExtension:url.pathExtension];
            NSString* unique = [folder stringByAppendingPathComponent:NSUUID.UUID.UUIDString];
            [NSFileManager.defaultManager createDirectoryAtPath:unique withIntermediateDirectories:YES attributes:nil error:nil];
            NSString* copy = [unique stringByAppendingPathComponent:name.lastPathComponent ?: @"file"];
            BOOL copied = url && [NSFileManager.defaultManager copyItemAtURL:url toURL:[NSURL fileURLWithPath:copy] error:nil];
            dispatch_async(dispatch_get_main_queue(), ^{
                if (copied) [paths addObject:copy];
                dispatch_group_leave(waiting);
            });
        }];
    }
    dispatch_group_notify(waiting, dispatch_get_main_queue(), ^{
        if (!paths.count || !g_ios_host.dropped) return;
        NSMutableData* list = [NSMutableData data];
        const char end = 0;
        for (NSString* path in paths) {
            [list appendBytes:path.fileSystemRepresentation length:strlen(path.fileSystemRepresentation) + 1];
        }
        [list appendBytes:&end length:1];
        g_ios_host.dropped(list.bytes, (float)at.x, (float)at.y);
        [g_ios_view markDirty];
    });
}

@end

void ng_ios_drop_attach(UIView* view) {
    /* the interaction keeps its delegate weak, the one view keeps this one */
    static AureaDropDelegate* delegate;
    if (!delegate) delegate = [[AureaDropDelegate alloc] init];
    [view addInteraction:[[UIDropInteraction alloc] initWithDelegate:delegate]];
}
