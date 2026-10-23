#import "../elements.h"
#import "../utils.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

/* Enter on the selected row activates it, as a double click does. */
@interface NGTableRows : NSTableView
@property (assign) unsigned int tableId;
@end

@implementation NGTableRows
- (void)keyDown:(NSEvent*)event {
    NSString* key = event.charactersIgnoringModifiers;
    unichar c = key.length == 1 ? [key characterAtIndex:0] : 0;
    if ((c == NSCarriageReturnCharacter || c == NSEnterCharacter) && self.selectedRow >= 0) {
        ng_invoke_table_activated(_tableId, (int)self.selectedRow);
        return;
    }
    [super keyDown:event];
}
@end

@interface NGTableView : NSScrollView <NSTableViewDataSource, NSTableViewDelegate>
@property (assign) unsigned int tableId;
@property (strong) NGTableRows* table;
@property (strong) NSMutableArray<NSArray<NSString*>*>* rows;
@property (assign) NSInteger selected;
@property (assign) BOOL quiet;
@end

@implementation NGTableView

- (instancetype)initWithId:(unsigned int)tableId {
    self = [super initWithFrame:NSMakeRect(0, 0, 300, 200)];
    if (self) {
        _tableId = tableId;
        _rows = [NSMutableArray array];
        _selected = -1;
        _table = [[NGTableRows alloc] initWithFrame:NSZeroRect];
        _table.tableId = tableId;
        _table.allowsMultipleSelection = NO;
        _table.allowsEmptySelection = YES;
        _table.usesAlternatingRowBackgroundColors = YES;
        _table.columnAutoresizingStyle = NSTableViewUniformColumnAutoresizingStyle;
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
    }
    return self;
}

- (NSInteger)numberOfRowsInTableView:(NSTableView*)tableView {
    (void)tableView;
    return (NSInteger)_rows.count;
}

- (id)tableView:(NSTableView*)tableView objectValueForTableColumn:(NSTableColumn*)column row:(NSInteger)row {
    if (row < 0 || row >= (NSInteger)_rows.count) return @"";
    NSInteger index = [tableView.tableColumns indexOfObject:column];
    NSArray<NSString*>* cells = _rows[(NSUInteger)row];
    return index >= 0 && index < (NSInteger)cells.count ? cells[(NSUInteger)index] : @"";
}

- (void)tableViewSelectionDidChange:(NSNotification*)notification {
    (void)notification;
    if (_quiet) return;
    NSInteger row = _table.selectedRow;
    if (row != _selected) {
        _selected = row;
        ng_invoke_table_selected(_tableId, (int)row);
    }
}

- (void)activate:(id)sender {
    (void)sender;
    NSInteger row = _table.clickedRow;
    if (row >= 0) ng_invoke_table_activated(_tableId, (int)row);
}

/* Changes made by the app are not reported. */
- (void)reload {
    _quiet = YES;
    [_table reloadData];
    _quiet = NO;
    _selected = _table.selectedRow;
}

@end

static NGTableView* table_of(NGHandle table) {
    if (!table) return nil;
    id view = (__bridge id)table;
    return [view isKindOfClass:[NGTableView class]] ? (NGTableView*)view : nil;
}

static NSArray<NSString*>* row_of(NGTableView* view, const char* const* cells, int count) {
    NSMutableArray<NSString*>* row = [NSMutableArray array];
    for (NSInteger i = 0; i < (NSInteger)view.table.tableColumns.count; i++) {
        const char* cell = i < count && cells[i] ? cells[i] : "";
        [row addObject:ng_macos_to_nsstring(cell) ?: @""];
    }
    return row;
}

static int check_cells(NGTableView* view, const char* const* cells, int count) {
    if (count < 0 || count > (int)view.table.tableColumns.count || (count > 0 && !cells)) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    return NG_SUCCESS;
}

NGHandle ng_macos_create_table(unsigned int id) {
    @autoreleasepool {
        NGTableView* view = [[NGTableView alloc] initWithId:id];
        return (__bridge_retained void*)view;
    }
}

int ng_macos_table_add_column(NGHandle table, const char* title) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (!title || view.rows.count > 0) return NG_ERROR_INVALID_PARAMETER;
    NSString* identifier = [NSString stringWithFormat:@"%lu", (unsigned long)view.table.tableColumns.count];
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:identifier];
    column.title = ng_macos_to_nsstring(title) ?: @"";
    column.resizingMask = NSTableColumnAutoresizingMask | NSTableColumnUserResizingMask;
    [view.table addTableColumn:column];
    [view.table sizeToFit];
    return NG_SUCCESS;
}

int ng_macos_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index > (int)view.rows.count || check_cells(view, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    NSInteger was = view.table.selectedRow;
    [view.rows insertObject:row_of(view, cells, count) atIndex:(NSUInteger)index];
    view.quiet = YES;
    [view.table reloadData];
    if (was >= index) {
        [view.table selectRowIndexes:[NSIndexSet indexSetWithIndex:(NSUInteger)(was + 1)]
                byExtendingSelection:NO];
    }
    view.quiet = NO;
    view.selected = view.table.selectedRow;
    return NG_SUCCESS;
}

int ng_macos_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.rows.count || check_cells(view, cells, count) != NG_SUCCESS) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    view.rows[(NSUInteger)index] = row_of(view, cells, count);
    [view reload];
    return NG_SUCCESS;
}

int ng_macos_table_remove_row(NGHandle table, int index) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.rows.count) return NG_ERROR_INVALID_PARAMETER;
    NSInteger was = view.table.selectedRow;
    [view.rows removeObjectAtIndex:(NSUInteger)index];
    view.quiet = YES;
    [view.table reloadData];
    if (was == index) {
        [view.table deselectAll:nil];
    } else if (was > index) {
        [view.table selectRowIndexes:[NSIndexSet indexSetWithIndex:(NSUInteger)(was - 1)]
                byExtendingSelection:NO];
    }
    view.quiet = NO;
    view.selected = view.table.selectedRow;
    return NG_SUCCESS;
}

int ng_macos_table_clear(NGHandle table) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    [view.rows removeAllObjects];
    view.quiet = YES;
    [view.table deselectAll:nil];
    view.quiet = NO;
    [view reload];
    return NG_SUCCESS;
}

int ng_macos_table_set_selected(NGHandle table, int index) {
    NGTableView* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= (int)view.rows.count) return NG_ERROR_INVALID_PARAMETER;
    view.quiet = YES;
    if (index < 0) {
        [view.table deselectAll:nil];
    } else {
        [view.table selectRowIndexes:[NSIndexSet indexSetWithIndex:(NSUInteger)index] byExtendingSelection:NO];
        [view.table scrollRowToVisible:index];
    }
    view.quiet = NO;
    view.selected = view.table.selectedRow;
    return NG_SUCCESS;
}

int ng_macos_table_get_selected(NGHandle table) {
    NGTableView* view = table_of(table);
    return view ? (int)view.table.selectedRow : -1;
}
