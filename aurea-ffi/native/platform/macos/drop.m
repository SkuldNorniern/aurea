#import "drop.h"
#import "content_view.h"
#import "common/input.h"
#import "common/types.h"
#import "common/rust_callbacks.h"

static NSString* drag_paths(id<NSDraggingInfo> info) {
    NSArray<NSURL*>* urls = [[info draggingPasteboard]
        readObjectsForClasses:@[[NSURL class]]
                      options:@{NSPasteboardURLReadingFileURLsOnlyKey: @YES}];
    NSMutableArray<NSString*>* paths = [NSMutableArray array];
    for (NSURL* url in urls) {
        if (url.path) [paths addObject:url.path];
    }
    return paths.count > 0 ? [paths componentsJoinedByString:@"\n"] : nil;
}

static BOOL report(AureaContentView* view, id<NSDraggingInfo> info, unsigned int kind) {
    NSString* paths = drag_paths(info);
    if (!paths) return NO;
    NSPoint p = [view convertPoint:[info draggingLocation] fromView:nil];
    CGFloat h = [view bounds].size.height;
    ng_invoke_file_drop(view.windowHandle, kind, p.x, h - p.y, [paths UTF8String]);
    return YES;
}

void ng_macos_drop_register(AureaContentView* view) {
    [view registerForDraggedTypes:@[NSPasteboardTypeFileURL]];
}

NSDragOperation ng_macos_drop_hover(AureaContentView* view, id<NSDraggingInfo> info) {
    return report(view, info, NG_DROP_HOVER) ? NSDragOperationCopy : NSDragOperationNone;
}

void ng_macos_drop_leave(AureaContentView* view) {
    ng_invoke_file_drop(view.windowHandle, NG_DROP_LEAVE, 0, 0, "");
}

BOOL ng_macos_drop_perform(AureaContentView* view, id<NSDraggingInfo> info) {
    return report(view, info, NG_DROP_DROP);
}
