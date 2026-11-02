#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

/* Rows of text under column titles, in equal columns. Tapping the selected row activates it, where
   a desktop double clicks. Changes the app makes are not reported. */

#define ROW_HEIGHT 44.0
#define HEADER_HEIGHT 32.0

static UIStackView* cells_view(NSArray<NSString*>* cells, BOOL header) {
    UIStackView* stack = [[UIStackView alloc] init];
    stack.axis = UILayoutConstraintAxisHorizontal;
    stack.distribution = UIStackViewDistributionFillEqually;
    stack.spacing = 8;
    stack.layoutMarginsRelativeArrangement = YES;
    stack.layoutMargins = UIEdgeInsetsMake(0, 16, 0, 16);
    for (NSString* cell in cells) {
        UILabel* label = [[UILabel alloc] init];
        label.text = cell;
        if (header) {
            label.font = [UIFont systemFontOfSize:UIFont.smallSystemFontSize weight:UIFontWeightSemibold];
            label.textColor = UIColor.secondaryLabelColor;
        }
        [stack addArrangedSubview:label];
    }
    return stack;
}

@interface NGIOSTable : UITableView <UITableViewDataSource, UITableViewDelegate>
@property (assign) unsigned int tableId;
@property (strong) NSMutableArray<NSString*>* columns;
@property (strong) NSMutableArray<NSArray<NSString*>*>* rows;
@property (assign) NSInteger selected;
@end

@implementation NGIOSTable

- (instancetype)initWithId:(unsigned int)tableId {
    self = [super initWithFrame:CGRectZero style:UITableViewStylePlain];
    if (self) {
        _tableId = tableId;
        _columns = [NSMutableArray array];
        _rows = [NSMutableArray array];
        _selected = -1;
        self.dataSource = self;
        self.delegate = self;
        self.rowHeight = ROW_HEIGHT;
        self.tableFooterView = [[UIView alloc] init];
    }
    return self;
}

- (NSInteger)tableView:(UITableView*)table numberOfRowsInSection:(NSInteger)section {
    (void)table; (void)section;
    return (NSInteger)_rows.count;
}

- (UITableViewCell*)tableView:(UITableView*)table cellForRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    UITableViewCell* cell = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
    UIStackView* stack = cells_view(_rows[(NSUInteger)path.row], NO);
    stack.frame = cell.contentView.bounds;
    stack.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [cell.contentView addSubview:stack];
    return cell;
}

- (CGFloat)tableView:(UITableView*)table heightForHeaderInSection:(NSInteger)section {
    (void)table; (void)section;
    return HEADER_HEIGHT;
}

- (UIView*)tableView:(UITableView*)table viewForHeaderInSection:(NSInteger)section {
    (void)table; (void)section;
    UIStackView* stack = cells_view(_columns, YES);
    stack.backgroundColor = UIColor.secondarySystemBackgroundColor;
    return stack;
}

- (void)tableView:(UITableView*)table didSelectRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    NSInteger row = path.row;
    if (row == _selected) {
        ng_invoke_table_activated(_tableId, (int)row);
    } else {
        _selected = row;
        ng_invoke_table_selected(_tableId, (int)row);
    }
}

- (void)showSelected {
    [self reloadData];
    if (_selected >= 0) {
        [self selectRowAtIndexPath:[NSIndexPath indexPathForRow:_selected inSection:0]
                          animated:NO
                    scrollPosition:UITableViewScrollPositionNone];
    }
}

@end

static NGIOSTable* table_of(NGHandle table) {
    if (!table) return nil;
    id view = (__bridge id)table;
    return [view isKindOfClass:[NGIOSTable class]] ? view : nil;
}

static NSArray<NSString*>* row_of(NGIOSTable* view, const char* const* cells, int count) {
    NSMutableArray<NSString*>* row = [NSMutableArray array];
    for (int i = 0; i < (int)view.columns.count; i++) {
        const char* cell = i < count && cells[i] ? cells[i] : "";
        [row addObject:[NSString stringWithUTF8String:cell] ?: @""];
    }
    return row;
}

static BOOL cells_fit(NGIOSTable* view, const char* const* cells, int count) {
    return count >= 0 && count <= (int)view.columns.count && (count == 0 || cells);
}

NGHandle ng_platform_create_table(unsigned int id) {
    return (__bridge_retained void*)[[NGIOSTable alloc] initWithId:id];
}

int ng_platform_table_add_column(NGHandle table, const char* title) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (!title || view.rows.count > 0) return NG_ERROR_INVALID_PARAMETER;
    [view.columns addObject:@(title)];
    [view reloadData];
    return NG_SUCCESS;
}

int ng_platform_table_insert_row(NGHandle table, int index, const char* const* cells, int count) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index > (int)view.rows.count || !cells_fit(view, cells, count)) return NG_ERROR_INVALID_PARAMETER;
    [view.rows insertObject:row_of(view, cells, count) atIndex:(NSUInteger)index];
    if (view.selected >= index) view.selected++;
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_table_set_row(NGHandle table, int index, const char* const* cells, int count) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.rows.count || !cells_fit(view, cells, count)) return NG_ERROR_INVALID_PARAMETER;
    view.rows[(NSUInteger)index] = row_of(view, cells, count);
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_table_remove_row(NGHandle table, int index) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.rows.count) return NG_ERROR_INVALID_PARAMETER;
    [view.rows removeObjectAtIndex:(NSUInteger)index];
    if (view.selected == index) view.selected = -1;
    else if (view.selected > index) view.selected--;
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_table_clear(NGHandle table) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    [view.rows removeAllObjects];
    view.selected = -1;
    [view reloadData];
    return NG_SUCCESS;
}

int ng_platform_table_set_selected(NGHandle table, int index) {
    NGIOSTable* view = table_of(table);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= (int)view.rows.count) return NG_ERROR_INVALID_PARAMETER;
    view.selected = index;
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_table_get_selected(NGHandle table) {
    NGIOSTable* view = table_of(table);
    return view ? (int)view.selected : -1;
}
