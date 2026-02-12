#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

extern void ng_invoke_sidebar_list_selected(unsigned int id, int index);

#define ROW_HEIGHT 18
#define INDENT_STEP 10
#define SECTION_PADDING 4

typedef NS_ENUM(NSInteger, NGSidebarRowType) {
    NGSidebarRowTypeSection,
    NGSidebarRowTypeItem
};

@interface NGSidebarRow : NSObject
@property (assign) NGSidebarRowType type;
@property (copy) NSString* title;
@property (assign) NSInteger indent;
@property (assign) NSInteger itemIndex;
@end

@implementation NGSidebarRow
@end

@interface NGSidebarListView : NSView
@property (assign) unsigned int sidebarId;
@property (strong) NSMutableArray<NGSidebarRow*>* rows;
@property (assign) NSInteger selectedIndex;
@property (assign) NSInteger nextItemIndex;
@end

@implementation NGSidebarListView
@synthesize selectedIndex = _selectedIndex;
- (instancetype)initWithFrame:(NSRect)frameRect id:(unsigned int)id {
    self = [super initWithFrame:frameRect];
    if (self) {
        _sidebarId = id;
        _rows = [NSMutableArray array];
        _selectedIndex = -1;
        _nextItemIndex = 0;
    }
    return self;
}

- (BOOL)isFlipped {
    return YES;
}

- (void)addSection:(NSString*)title {
    NGSidebarRow* row = [[NGSidebarRow alloc] init];
    row.type = NGSidebarRowTypeSection;
    row.title = title ?: @"";
    row.indent = 0;
    row.itemIndex = -1;
    [_rows addObject:row];
}

- (void)addItem:(NSString*)title indent:(NSInteger)indent {
    NGSidebarRow* row = [[NGSidebarRow alloc] init];
    row.type = NGSidebarRowTypeItem;
    row.title = title ?: @"";
    row.indent = indent;
    row.itemIndex = _nextItemIndex++;
    [_rows addObject:row];
}

- (void)setSelectedIndex:(NSInteger)index {
    _selectedIndex = index;
    [self setNeedsDisplay:YES];
}

- (NSInteger)selectedIndex {
    return _selectedIndex;
}

- (void)clear {
    [_rows removeAllObjects];
    _nextItemIndex = 0;
    _selectedIndex = -1;
    [self setNeedsDisplay:YES];
}

- (NSInteger)itemIndexAtPoint:(NSPoint)point {
    CGFloat y = 2;
    for (NGSidebarRow* row in _rows) {
        CGFloat rowHeight = (row.type == NGSidebarRowTypeSection) ? ROW_HEIGHT + SECTION_PADDING : ROW_HEIGHT;
        if (point.y >= y && point.y < y + rowHeight) {
            if (row.type == NGSidebarRowTypeItem) return row.itemIndex;
            return -1;
        }
        y += rowHeight;
    }
    return -1;
}

- (void)drawRect:(NSRect)dirtyRect {
    [[NSColor windowBackgroundColor] setFill];
    NSRectFill(self.bounds);

    NSDictionary* sectionAttrs = @{
        NSFontAttributeName: [NSFont boldSystemFontOfSize:10],
        NSForegroundColorAttributeName: [NSColor secondaryLabelColor]
    };
    NSDictionary* itemAttrs = @{
        NSFontAttributeName: [NSFont systemFontOfSize:12],
        NSForegroundColorAttributeName: [NSColor textColor]
    };
    NSDictionary* selectedAttrs = @{
        NSFontAttributeName: [NSFont systemFontOfSize:12],
        NSForegroundColorAttributeName: [NSColor controlTextColor]
    };

    CGFloat y = 2;
    for (NGSidebarRow* row in _rows) {
        if (y + ROW_HEIGHT > 0 && y < self.bounds.size.height) {
            if (row.type == NGSidebarRowTypeSection) {
                [row.title drawAtPoint:NSMakePoint(6, y + 4) withAttributes:sectionAttrs];
            } else {
                BOOL selected = (row.itemIndex == _selectedIndex);
                if (selected) {
                    NSRect rowRect = NSMakeRect(0, y, self.bounds.size.width, ROW_HEIGHT);
                    [[NSColor selectedContentBackgroundColor] setFill];
                    NSRectFill(rowRect);
                }
                CGFloat indent = 6 + row.indent * INDENT_STEP;
                NSDictionary* attrs = selected ? selectedAttrs : itemAttrs;
                [row.title drawAtPoint:NSMakePoint(indent, y + 2) withAttributes:attrs];
            }
        }
        y += ROW_HEIGHT;
        if (row.type == NGSidebarRowTypeSection) {
            y += SECTION_PADDING;
        }
    }
}

- (void)mouseDown:(NSEvent*)event {
    NSPoint loc = [self convertPoint:[event locationInWindow] fromView:nil];
    NSInteger idx = [self itemIndexAtPoint:loc];
    if (idx >= 0) {
        _selectedIndex = idx;
        [self setNeedsDisplay:YES];
        ng_invoke_sidebar_list_selected(_sidebarId, (int)idx);
    }
}
@end

NGHandle ng_macos_create_sidebar_list(unsigned int id) {
    @autoreleasepool {
        NGSidebarListView* view = [[NGSidebarListView alloc] initWithFrame:NSZeroRect id:id];
        [view setTranslatesAutoresizingMaskIntoConstraints:NO];
        return (__bridge_retained void*)view;
    }
}

int ng_macos_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    [view addSection:ng_macos_to_nsstring(title)];
    return NG_SUCCESS;
}

int ng_macos_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    [view addItem:ng_macos_to_nsstring(title) indent:indent];
    return NG_SUCCESS;
}

int ng_macos_sidebar_list_set_selected(NGHandle sidebar, int index) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    [view setSelectedIndex:index];
    return NG_SUCCESS;
}

int ng_macos_sidebar_list_get_selected(NGHandle sidebar) {
    if (!sidebar) return -1;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    return (int)[view selectedIndex];
}

int ng_macos_sidebar_list_clear(NGHandle sidebar) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    [view clear];
    return NG_SUCCESS;
}

void ng_macos_sidebar_list_invalidate(NGHandle sidebar) {
    if (!sidebar) return;
    NGSidebarListView* view = (__bridge NGSidebarListView*)sidebar;
    [view setNeedsDisplay:YES];
}
