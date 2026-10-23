#import "../elements.h"
#import "../utils.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

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

@class NGSidebarListView;

@interface NGSidebarAXRow : NSAccessibilityElement
@property (weak) NGSidebarListView* owner;
@property (strong) NGSidebarRow* row;
@property (assign) NSUInteger position;
@end

@interface NGSidebarListView : NSView
@property (assign) unsigned int sidebarId;
@property (strong) NSArray<NGSidebarAXRow*>* axRows;
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
    [self rowsChanged:row];
}

- (void)rowsChanged:(NGSidebarRow*)added {
    if (added) [_rows addObject:added];
    _axRows = nil;
    [self invalidateIntrinsicContentSize];
    [self setNeedsDisplay:YES];
    NSAccessibilityPostNotification(self, NSAccessibilityLayoutChangedNotification);
}

- (void)addItem:(NSString*)title indent:(NSInteger)indent {
    NGSidebarRow* row = [[NGSidebarRow alloc] init];
    row.type = NGSidebarRowTypeItem;
    row.title = title ?: @"";
    row.indent = MAX(indent, 0);
    row.itemIndex = _nextItemIndex++;
    [self rowsChanged:row];
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
    [self rowsChanged:nil];
}

- (CGFloat)heightOfRow:(NGSidebarRow*)row {
    return row.type == NGSidebarRowTypeSection ? ROW_HEIGHT + SECTION_PADDING : ROW_HEIGHT;
}

- (NSRect)rectOfRowAt:(NSUInteger)position {
    CGFloat y = 2;
    for (NSUInteger i = 0; i < position && i < _rows.count; i++) y += [self heightOfRow:_rows[i]];
    CGFloat height = position < _rows.count ? [self heightOfRow:_rows[position]] : 0;
    return NSMakeRect(0, y, self.bounds.size.width, height);
}

- (NSSize)intrinsicContentSize {
    return NSMakeSize(NSViewNoIntrinsicMetric, NSMaxY([self rectOfRowAt:_rows.count]) + 2);
}

- (void)selectItem:(NSInteger)index {
    _selectedIndex = index;
    [self setNeedsDisplay:YES];
    for (NSUInteger i = 0; i < _rows.count; i++)
        if (_rows[i].itemIndex == index) [self scrollRectToVisible:[self rectOfRowAt:i]];
    NSAccessibilityPostNotification(self, NSAccessibilitySelectedChildrenChangedNotification);
    ng_invoke_sidebar_list_selected(_sidebarId, (int)index);
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (NSRect)focusRingMaskBounds {
    return self.visibleRect;
}

- (void)drawFocusRingMask {
    NSRectFill(self.visibleRect);
}

- (void)keyDown:(NSEvent*)event {
    NSString* chars = event.charactersIgnoringModifiers;
    unichar key = chars.length ? [chars characterAtIndex:0] : 0;
    NSInteger last = _nextItemIndex - 1, target;
    if (key == NSHomeFunctionKey) target = 0;
    else if (key == NSEndFunctionKey) target = last;
    else if (key == NSUpArrowFunctionKey) target = _selectedIndex < 0 ? last : _selectedIndex - 1;
    else if (key == NSDownArrowFunctionKey) target = _selectedIndex + 1;
    else { [super keyDown:event]; return; }
    target = MIN(MAX(target, 0), last);
    if (target >= 0 && target != _selectedIndex) [self selectItem:target];
}

- (BOOL)isAccessibilityElement {
    return YES;
}

- (NSAccessibilityRole)accessibilityRole {
    return NSAccessibilityListRole;
}

- (NSArray*)accessibilityChildren {
    if (!_axRows) {
        NSMutableArray* rows = [NSMutableArray arrayWithCapacity:_rows.count];
        for (NSUInteger i = 0; i < _rows.count; i++) {
            NGSidebarAXRow* element = [[NGSidebarAXRow alloc] init];
            element.owner = self;
            element.row = _rows[i];
            element.position = i;
            element.accessibilityParent = self;
            [rows addObject:element];
        }
        _axRows = rows;
    }
    return _axRows;
}

- (NSArray*)accessibilitySelectedChildren {
    for (NGSidebarAXRow* element in [self accessibilityChildren])
        if (element.row.type == NGSidebarRowTypeItem && element.row.itemIndex == _selectedIndex) return @[element];
    return @[];
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
        NSForegroundColorAttributeName: [NSColor alternateSelectedControlTextColor]
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
                    if (@available(macOS 10.14, *)) {
                        [[NSColor selectedContentBackgroundColor] setFill];
                    } else {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
                        [[NSColor alternateSelectedControlColor] setFill];
#pragma clang diagnostic pop
                    }
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
    [self.window makeFirstResponder:self];
    if (idx >= 0) [self selectItem:idx];
}
@end

@implementation NGSidebarAXRow
- (NSAccessibilityRole)accessibilityRole {
    return NSAccessibilityStaticTextRole;
}

- (id)accessibilityValue {
    return self.row.title;
}

- (NSRect)accessibilityFrame {
    NGSidebarListView* owner = self.owner;
    return owner ? NSAccessibilityFrameInView(owner, [owner rectOfRowAt:self.position]) : NSZeroRect;
}

- (BOOL)isAccessibilitySelected {
    return self.row.type == NGSidebarRowTypeItem && self.row.itemIndex == self.owner.selectedIndex;
}

- (BOOL)accessibilityPerformPress {
    NGSidebarListView* owner = self.owner;
    if (!owner || self.row.type != NGSidebarRowTypeItem) return NO;
    [owner selectItem:self.row.itemIndex];
    return YES;
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
    if (index < -1 || index >= view.nextItemIndex) return NG_ERROR_INVALID_PARAMETER;
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
