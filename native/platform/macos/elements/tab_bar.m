#import "../elements.h"
#import "../utils.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

#define TAB_HEIGHT 28
#define TAB_PADDING 8
#define DRAG_THRESHOLD 30

@interface NGTabBarView : NSView
@property (assign) unsigned int tabBarId;
@property (strong) NSMutableArray<NSString*>* tabTitles;
@property (assign) NSInteger selectedIndex;
@property (assign) NSInteger dragStartIndex;
@property (assign) BOOL isDragging;
@property (assign) NSPoint dragStartPoint;
@end

@implementation NGTabBarView
@synthesize selectedIndex = _selectedIndex;

- (instancetype)initWithFrame:(NSRect)frameRect id:(unsigned int)id {
    self = [super initWithFrame:frameRect];
    if (self) {
        _tabBarId = id;
        _tabTitles = [NSMutableArray array];
        _selectedIndex = 0;
        _dragStartIndex = -1;
        _isDragging = NO;
    }
    return self;
}

- (BOOL)isFlipped {
    return YES;
}

- (NSSize)intrinsicContentSize {
    return NSMakeSize(NSViewNoIntrinsicMetric, TAB_HEIGHT);
}

- (void)addTabWithTitle:(NSString*)title {
    [_tabTitles addObject:title ?: @""];
    [self setNeedsDisplay:YES];
}

- (void)removeTabAtIndex:(NSInteger)index {
    if (index >= 0 && index < (NSInteger)_tabTitles.count) {
        [_tabTitles removeObjectAtIndex:index];
        if (_selectedIndex >= (NSInteger)_tabTitles.count) {
            _selectedIndex = _tabTitles.count > 0 ? _tabTitles.count - 1 : 0;
        }
        if (_selectedIndex < 0) _selectedIndex = 0;
        [self setNeedsDisplay:YES];
    }
}

- (void)setSelectedIndex:(NSInteger)index {
    if (index >= 0 && index < (NSInteger)_tabTitles.count) {
        _selectedIndex = index;
        [self setNeedsDisplay:YES];
    }
}

- (NSInteger)selectedIndex {
    return _selectedIndex;
}

- (NSInteger)segmentAtPoint:(NSPoint)point {
    if (_tabTitles.count == 0) return -1;
    CGFloat x = TAB_PADDING;
    CGFloat segWidth = ([self bounds].size.width - TAB_PADDING * 2) / (CGFloat)_tabTitles.count;
    for (NSUInteger i = 0; i < _tabTitles.count; i++) {
        if (point.x >= x && point.x < x + segWidth && point.y >= 0 && point.y < TAB_HEIGHT) {
            return (NSInteger)i;
        }
        x += segWidth;
    }
    return -1;
}

- (void)drawRect:(NSRect)dirtyRect {
    [[NSColor windowBackgroundColor] setFill];
    NSRectFill(self.bounds);

    if (_tabTitles.count == 0) return;

    NSDictionary* normalAttrs = @{
        NSFontAttributeName: [NSFont systemFontOfSize:12],
        NSForegroundColorAttributeName: [NSColor textColor]
    };
    NSDictionary* selectedAttrs = @{
        NSFontAttributeName: [NSFont systemFontOfSize:12 weight:NSFontWeightMedium],
        NSForegroundColorAttributeName: [NSColor textColor]
    };

    CGFloat x = TAB_PADDING;
    CGFloat segWidth = ([self bounds].size.width - TAB_PADDING * 2) / (CGFloat)_tabTitles.count;

    for (NSUInteger i = 0; i < _tabTitles.count; i++) {
        NSRect segRect = NSMakeRect(x, 2, segWidth - 2, TAB_HEIGHT - 4);
        BOOL selected = ((NSInteger)i == _selectedIndex);
        NSBezierPath* path = [NSBezierPath bezierPathWithRoundedRect:segRect xRadius:4 yRadius:4];
        if (selected) {
            [[NSColor controlBackgroundColor] setFill];
            [path fill];
            [[NSColor separatorColor] setStroke];
            [path setLineWidth:1];
            [path stroke];
        }
        NSString* title = _tabTitles[i];
        if ([title length] > 0) {
            NSDictionary* attrs = selected ? selectedAttrs : normalAttrs;
            NSSize textSize = [title sizeWithAttributes:attrs];
            NSPoint textOrigin = NSMakePoint(
                segRect.origin.x + (segRect.size.width - textSize.width) / 2,
                segRect.origin.y + (segRect.size.height - textSize.height) / 2
            );
            [title drawAtPoint:textOrigin withAttributes:attrs];
        }
        x += segWidth;
    }
}

- (void)mouseDown:(NSEvent*)event {
    NSPoint loc = [self convertPoint:[event locationInWindow] fromView:nil];
    NSInteger seg = [self segmentAtPoint:loc];
    if (seg >= 0) {
        _dragStartIndex = seg;
        _dragStartPoint = loc;
        _isDragging = NO;
    }
}

- (void)mouseDragged:(NSEvent*)event {
    if (_dragStartIndex < 0) return;
    NSPoint loc = [self convertPoint:[event locationInWindow] fromView:nil];
    CGFloat dx = loc.x - _dragStartPoint.x;
    CGFloat dy = loc.y - _dragStartPoint.y;
    CGFloat dist = sqrt(dx*dx + dy*dy);
    if (!_isDragging && dist > DRAG_THRESHOLD) {
        _isDragging = YES;
    }
}

- (void)mouseUp:(NSEvent*)event {
    if (_dragStartIndex < 0) return;
    NSPoint loc = [self convertPoint:[event locationInWindow] fromView:nil];
    NSPoint screenLoc = [self.window convertPointToScreen:[event locationInWindow]];
    NSRect winFrame = self.window.frame;

    if (_isDragging) {
        if (screenLoc.x < winFrame.origin.x || screenLoc.x > winFrame.origin.x + winFrame.size.width ||
            screenLoc.y < winFrame.origin.y || screenLoc.y > winFrame.origin.y + winFrame.size.height) {
            ng_invoke_tab_bar_detach(_tabBarId, (int)_dragStartIndex);
        }
    } else {
        NSInteger seg = [self segmentAtPoint:loc];
        if (seg >= 0 && seg != _selectedIndex) {
            _selectedIndex = seg;
            [self setNeedsDisplay:YES];
            ng_invoke_tab_bar_selected(_tabBarId, (int)seg);
        }
    }
    _dragStartIndex = -1;
    _isDragging = NO;
}
@end

typedef struct {
    NGTabBarView* view;
} TabBarData;

NGHandle ng_macos_create_tab_bar(unsigned int id) {
    @autoreleasepool {
        NGTabBarView* view = [[NGTabBarView alloc] initWithFrame:NSZeroRect id:id];
        [view setTranslatesAutoresizingMaskIntoConstraints:NO];
        return (__bridge_retained void*)view;
    }
}

int ng_macos_tab_bar_add_tab(NGHandle tab_bar, const char* title) {
    if (!tab_bar || !title) return NG_ERROR_INVALID_PARAMETER;
    NGTabBarView* view = (__bridge NGTabBarView*)tab_bar;
    NSString* nsTitle = ng_macos_to_nsstring(title);
    [view addTabWithTitle:nsTitle];
    return NG_SUCCESS;
}

int ng_macos_tab_bar_remove_tab(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    NGTabBarView* view = (__bridge NGTabBarView*)tab_bar;
    [view removeTabAtIndex:index];
    return NG_SUCCESS;
}

int ng_macos_tab_bar_set_selected(NGHandle tab_bar, int index) {
    if (!tab_bar) return NG_ERROR_INVALID_HANDLE;
    NGTabBarView* view = (__bridge NGTabBarView*)tab_bar;
    [view setSelectedIndex:index];
    return NG_SUCCESS;
}

int ng_macos_tab_bar_get_selected(NGHandle tab_bar) {
    if (!tab_bar) return -1;
    NGTabBarView* view = (__bridge NGTabBarView*)tab_bar;
    return (int)[view selectedIndex];
}

void ng_macos_tab_bar_invalidate(NGHandle tab_bar) {
    if (!tab_bar) return;
    NGTabBarView* view = (__bridge NGTabBarView*)tab_bar;
    [view setNeedsDisplay:YES];
}
