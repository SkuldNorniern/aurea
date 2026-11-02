#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

/* A list whose rows are elements, as tall as each needs. Tapping the selected row activates it, where
   a desktop double clicks. Changes the app makes are not reported. */

#define ROW_PADDING 4.0

void ng_ios_notify_destroyed(UIView* view);

@interface NGIOSRowCell : UITableViewCell
@property (strong) UIView* element;
@end

@implementation NGIOSRowCell
- (instancetype)initWithElement:(UIView*)element {
    self = [super initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
    if (self) {
        _element = element;
        element.translatesAutoresizingMaskIntoConstraints = NO;
        [self.contentView addSubview:element];
        UILayoutGuide* margins = self.contentView.layoutMarginsGuide;
        [NSLayoutConstraint activateConstraints:@[
            [element.leadingAnchor constraintEqualToAnchor:margins.leadingAnchor],
            [element.trailingAnchor constraintEqualToAnchor:margins.trailingAnchor],
            [element.topAnchor constraintEqualToAnchor:self.contentView.topAnchor constant:ROW_PADDING],
            [element.bottomAnchor constraintEqualToAnchor:self.contentView.bottomAnchor constant:-ROW_PADDING],
        ]];
    }
    return self;
}
@end

@interface NGIOSRowList : UITableView <UITableViewDataSource, UITableViewDelegate>
@property (assign) unsigned int listId;
@property (strong) NSMutableArray<NGIOSRowCell*>* cells;
@property (assign) NSInteger selected;
@end

@implementation NGIOSRowList

- (instancetype)initWithId:(unsigned int)listId {
    self = [super initWithFrame:CGRectZero style:UITableViewStylePlain];
    if (self) {
        _listId = listId;
        _cells = [NSMutableArray array];
        _selected = -1;
        self.dataSource = self;
        self.delegate = self;
        self.rowHeight = UITableViewAutomaticDimension;
        self.estimatedRowHeight = 44;
        self.tableFooterView = [[UIView alloc] init];
    }
    return self;
}

- (NSInteger)tableView:(UITableView*)table numberOfRowsInSection:(NSInteger)section {
    (void)table; (void)section;
    return (NSInteger)_cells.count;
}

- (UITableViewCell*)tableView:(UITableView*)table cellForRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    return _cells[(NSUInteger)path.row];
}

- (void)tableView:(UITableView*)table didSelectRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    NSInteger row = path.row;
    if (row == _selected) {
        ng_invoke_row_list_activated(_listId, (int)row);
    } else {
        _selected = row;
        ng_invoke_row_list_selected(_listId, (int)row);
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

static NGIOSRowList* list_of(NGHandle list) {
    if (!list) return nil;
    id view = (__bridge id)list;
    return [view isKindOfClass:[NGIOSRowList class]] ? view : nil;
}

NGHandle ng_platform_create_row_list(unsigned int id) {
    return (__bridge_retained void*)[[NGIOSRowList alloc] initWithId:id];
}

int ng_platform_row_list_insert(NGHandle list, int index, NGHandle element) {
    NGIOSRowList* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (!element || index < 0 || index > (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    UIView* content = (__bridge UIView*)element;
    [content removeFromSuperview];
    [view.cells insertObject:[[NGIOSRowCell alloc] initWithElement:content] atIndex:(NSUInteger)index];
    if (view.selected >= index) view.selected++;
    [view showSelected];
    return NG_SUCCESS;
}

static void remove_rows(NGIOSRowList* view, NSIndexSet* rows) {
    NSArray<NGIOSRowCell*>* gone = [view.cells objectsAtIndexes:rows];
    [view.cells removeObjectsAtIndexes:rows];
    [view showSelected];
    for (NGIOSRowCell* cell in gone) {
        ng_ios_notify_destroyed(cell.element);
        [cell.element removeFromSuperview];
    }
}

int ng_platform_row_list_remove(NGHandle list, int index) {
    NGIOSRowList* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < 0 || index >= (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    if (view.selected == index) view.selected = -1;
    else if (view.selected > index) view.selected--;
    remove_rows(view, [NSIndexSet indexSetWithIndex:(NSUInteger)index]);
    return NG_SUCCESS;
}

int ng_platform_row_list_clear(NGHandle list) {
    NGIOSRowList* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    view.selected = -1;
    remove_rows(view, [NSIndexSet indexSetWithIndexesInRange:NSMakeRange(0, view.cells.count)]);
    return NG_SUCCESS;
}

int ng_platform_row_list_set_selected(NGHandle list, int index) {
    NGIOSRowList* view = list_of(list);
    if (!view) return NG_ERROR_INVALID_HANDLE;
    if (index < -1 || index >= (int)view.cells.count) return NG_ERROR_INVALID_PARAMETER;
    view.selected = index;
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_row_list_get_selected(NGHandle list) {
    NGIOSRowList* view = list_of(list);
    return view ? (int)view.selected : -1;
}
