#import "../elements.h"
#import "../window.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

#define ROW_PADDING 4

/* Holds one row's element. VoiceOver reads the row as its labels. */
@interface NGRowCell : NSTableCellView
@property (strong) NSView* element;
@end

static void ng_row_labels(NSView* view, NSMutableArray<NSString*>* into) {
    if (view.hidden) return;
    if ([view isKindOfClass:[NSTextField class]]) {
        NSTextField* field = (NSTextField*)view;
        if (!field.editable && field.stringValue.length > 0) [into addObject:field.stringValue];
        return;
    }
    for (NSView* sub in view.subviews) ng_row_labels(sub, into);
}

@implementation NGRowCell
- (instancetype)initWithElement:(NSView*)element {
    self = [super initWithFrame:NSMakeRect(0, 0, 100, 24)];
    if (self) {
        _element = element;
        element.translatesAutoresizingMaskIntoConstraints = NO;
        [self addSubview:element];
        [NSLayoutConstraint activateConstraints:@[
            [element.leadingAnchor constraintEqualToAnchor:self.leadingAnchor constant:ROW_PADDING],
            [element.trailingAnchor constraintEqualToAnchor:self.trailingAnchor constant:-ROW_PADDING],
            [element.topAnchor constraintEqualToAnchor:self.topAnchor constant:ROW_PADDING],
            [element.bottomAnchor constraintEqualToAnchor:self.bottomAnchor constant:-ROW_PADDING],
        ]];
    }
    return self;
}

- (NSString*)accessibilityLabel {
    NSMutableArray<NSString*>* labels = [NSMutableArray array];
    ng_row_labels(_element, labels);
    return [labels componentsJoinedByString:@", "];
}
@end

@interface NGRowListRows : NSTableView
@property (assign) unsigned int listId;
@end

/* Enter on the selected row activates it, as a double click does. */
@implementation NGRowListRows
- (void)keyDown:(NSEvent*)event {
    NSString* key = event.charactersIgnoringModifiers;
    unichar c = key.length == 1 ? [key characterAtIndex:0] : 0;
    if ((c == NSCarriageReturnCharacter || c == NSEnterCharacter) && self.selectedRow >= 0) {
        ng_invoke_row_list_activated(_listId, (int)self.selectedRow);
        return;
    }
    [super keyDown:event];
}
@end

@interface NGRowListView : NSScrollView <NSTableViewDataSource, NSTableViewDelegate>
@property (assign) unsigned int listId;
@property (strong) NGRowListRows* table;
@property (strong) NSMutableArray<NGRowCell*>* cells;
@property (assign) NSInteger selected;
@property (assign) BOOL quiet;
@property (assign) BOOL automaticHeights;
@end

@implementation NGRowListView

- (instancetype)initWithId:(unsigned int)listId {
    self = [super initWithFrame:NSMakeRect(0, 0, 300, 200)];
    if (self) {
        _listId = listId;
        _cells = [NSMutableArray array];
        _selected = -1;
        _table = [[NGRowListRows alloc] initWithFrame:NSZeroRect];
        _table.listId = listId;
        NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"row"];
        column.resizingMask = NSTableColumnAutoresizingMask;
        [_table addTableColumn:column];
        _table.headerView = nil;
        _table.allowsMultipleSelection = NO;
        _table.allowsEmptySelection = YES;
        _table.columnAutoresizingStyle = NSTableViewLastColumnOnlyAutoresizingStyle;
        if (@available(macOS 10.13, *)) {
            _table.usesAutomaticRowHeights = YES;
            _automaticHeights = YES;
        }
        _table.dataSource = self;
        _table.delegate = self;
        _table.target = self;
        _table.doubleAction = @selector(activate:);
        self.documentView = _table;
        self.hasVerticalScroller = YES;
        self.hasHorizontalScroller = NO;
        self.autohidesScrollers = YES;
        self.borderType = NSBezelBorder;
        self.translatesAutoresizingMaskIntoConstraints = NO;
        [_table sizeLastColumnToFit];
    }
    return self;
}

- (NSSize)intrinsicContentSize {
    return NSMakeSize(NSViewNoIntrinsicMetric, 120);
}

/* Before 10.13 rows are as tall as their element fits. */
- (BOOL)respondsToSelector:(SEL)selector {
    if (selector == @selector(tableView:heightOfRow:)) return !_automaticHeights;
    return [super respondsToSelector:selector];
}

- (CGFloat)tableView:(NSTableView*)tableView heightOfRow:(NSInteger)row {
    if (row < 0 || row >= (NSInteger)_cells.count) return tableView.rowHeight;
    return MAX(_cells[(NSUInteger)row].fittingSize.height, 1);
}

- (NSInteger)numberOfRowsInTableView:(NSTableView*)tableView {
    (void)tableView;
    return (NSInteger)_cells.count;
}

- (NSView*)tableView:(NSTableView*)tableView viewForTableColumn:(NSTableColumn*)column row:(NSInteger)row {
    (void)tableView;
    (void)column;
    return row >= 0 && row < (NSInteger)_cells.count ? _cells[(NSUInteger)row] : nil;
}

- (void)tableViewSelectionDidChange:(NSNotification*)notification {
    (void)notification;
    if (_quiet) return;
    NSInteger row = _table.selectedRow;
    if (row != _selected) {
        _selected = row;
        ng_invoke_row_list_selected(_listId, (int)row);
    }
}

- (void)activate:(id)sender {
    (void)sender;
    NSInteger row = _table.clickedRow;
    if (row >= 0) ng_invoke_row_list_activated(_listId, (int)row);
}

/* Changes made by the app are not reported. */
- (void)quietly:(void (^)(void))change {
    _quiet = YES;
    change();
    _quiet = NO;
    _selected = _table.selectedRow;
}

@end

static NGRowListView* list_of(NGHandle list) {
    if (!list) return nil;
    id view = (__bridge id)list;
    return [view isKindOfClass:[NGRowListView class]] ? (NGRowListView*)view : nil;
}

NGHandle ng_macos_create_row_list(unsigned int id) {
    NGRowListView* view = [[NGRowListView alloc] initWithId:id];
    return (__bridge_retained void*)view;
}

int ng_macos_row_list_insert(NGHandle list, int index, NGHandle element) {
    NGRowListView* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (!element || index < 0 || index > (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    NSView* content = (__bridge NSView*)element;
    if (![content isKindOfClass:[NSView class]]) return NG_ERROR_INVALID_PARAMETER;
    [content removeFromSuperview];
    NGRowCell* cell = [[NGRowCell alloc] initWithElement:content];
    [view quietly:^{
        [view.cells insertObject:cell atIndex:(NSUInteger)index];
        [view.table insertRowsAtIndexes:[NSIndexSet indexSetWithIndex:(NSUInteger)index]
                          withAnimation:NSTableViewAnimationEffectNone];
    }];
    return NG_SUCCESS;
}

static void remove_rows(NGRowListView* view, NSIndexSet* rows) {
    NSArray<NGRowCell*>* gone = [view.cells objectsAtIndexes:rows];
    [view quietly:^{
        [view.cells removeObjectsAtIndexes:rows];
        [view.table removeRowsAtIndexes:rows withAnimation:NSTableViewAnimationEffectNone];
    }];
    for (NGRowCell* cell in gone) {
        ng_macos_element_destroyed((__bridge NGHandle)cell.element);
        [cell.element removeFromSuperview];
        [cell removeFromSuperview];
    }
}

int ng_macos_row_list_remove(NGHandle list, int index) {
    NGRowListView* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    remove_rows(view, [NSIndexSet indexSetWithIndex:(NSUInteger)index]);
    return NG_SUCCESS;
}

int ng_macos_row_list_clear(NGHandle list) {
    NGRowListView* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    remove_rows(view, [NSIndexSet indexSetWithIndexesInRange:NSMakeRange(0, view.cells.count)]);
    return NG_SUCCESS;
}

int ng_macos_row_list_set_selected(NGHandle list, int index) {
    NGRowListView* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    [view quietly:^{
        if (index < 0) {
            [view.table deselectAll:nil];
        } else {
            [view.table selectRowIndexes:[NSIndexSet indexSetWithIndex:(NSUInteger)index] byExtendingSelection:NO];
            [view.table scrollRowToVisible:index];
        }
    }];
    return NG_SUCCESS;
}

int ng_macos_row_list_get_selected(NGHandle list) {
    NGRowListView* view = list_of(list);
    return view ? (int)view.table.selectedRow : -1;
}
