#import "../elements.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

/* A list of section titles and items. Indexes count items only. */

@interface NGIOSSidebarRow : NSObject
@property (strong) NSString* title;
@property (assign) NSInteger indent;
@property (assign) NSInteger item;
@end

@implementation NGIOSSidebarRow
@end

@interface NGIOSSidebarList : UITableView <UITableViewDataSource, UITableViewDelegate>
@property (assign) unsigned int sidebarId;
@property (strong) NSMutableArray<NGIOSSidebarRow*>* rows;
@property (assign) NSInteger items;
@property (assign) NSInteger selected;
@end

@implementation NGIOSSidebarList

- (instancetype)initWithId:(unsigned int)sidebarId {
    self = [super initWithFrame:CGRectZero style:UITableViewStylePlain];
    if (self) {
        _sidebarId = sidebarId;
        _rows = [NSMutableArray array];
        _selected = -1;
        self.dataSource = self;
        self.delegate = self;
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
    NGIOSSidebarRow* row = _rows[(NSUInteger)path.row];
    UITableViewCell* cell = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
    cell.textLabel.text = row.title;
    if (row.item < 0) {
        cell.textLabel.font = [UIFont systemFontOfSize:UIFont.smallSystemFontSize weight:UIFontWeightSemibold];
        cell.textLabel.textColor = UIColor.secondaryLabelColor;
        cell.selectionStyle = UITableViewCellSelectionStyleNone;
    }
    cell.indentationLevel = row.indent;
    return cell;
}

- (NSIndexPath*)tableView:(UITableView*)table willSelectRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    return _rows[(NSUInteger)path.row].item >= 0 ? path : nil;
}

- (void)tableView:(UITableView*)table didSelectRowAtIndexPath:(NSIndexPath*)path {
    (void)table;
    _selected = _rows[(NSUInteger)path.row].item;
    ng_invoke_sidebar_list_selected(_sidebarId, (int)_selected);
}

- (void)showSelected {
    [self reloadData];
    for (NSUInteger i = 0; i < _rows.count; i++) {
        if (_selected >= 0 && _rows[i].item == _selected) {
            [self selectRowAtIndexPath:[NSIndexPath indexPathForRow:(NSInteger)i inSection:0]
                              animated:NO
                        scrollPosition:UITableViewScrollPositionNone];
        }
    }
}

@end

NGHandle ng_platform_create_sidebar_list(unsigned int id) {
    return (__bridge_retained void*)[[NGIOSSidebarList alloc] initWithId:id];
}

static void add_row(NGIOSSidebarList* view, const char* title, NSInteger indent, NSInteger item) {
    NGIOSSidebarRow* row = [[NGIOSSidebarRow alloc] init];
    row.title = @(title);
    row.indent = indent;
    row.item = item;
    [view.rows addObject:row];
    [view showSelected];
}

int ng_platform_sidebar_list_add_section(NGHandle sidebar, const char* title) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    add_row((__bridge NGIOSSidebarList*)sidebar, title, 0, -1);
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_add_item(NGHandle sidebar, const char* title, int indent) {
    if (!sidebar || !title) return NG_ERROR_INVALID_PARAMETER;
    NGIOSSidebarList* view = (__bridge NGIOSSidebarList*)sidebar;
    add_row(view, title, indent, view.items++);
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_set_selected(NGHandle sidebar, int index) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    NGIOSSidebarList* view = (__bridge NGIOSSidebarList*)sidebar;
    if (index < -1 || index >= view.items) return NG_ERROR_INVALID_PARAMETER;
    view.selected = index;
    [view showSelected];
    return NG_SUCCESS;
}

int ng_platform_sidebar_list_get_selected(NGHandle sidebar) {
    return sidebar ? (int)((__bridge NGIOSSidebarList*)sidebar).selected : -1;
}

int ng_platform_sidebar_list_clear(NGHandle sidebar) {
    if (!sidebar) return NG_ERROR_INVALID_HANDLE;
    NGIOSSidebarList* view = (__bridge NGIOSSidebarList*)sidebar;
    [view.rows removeAllObjects];
    view.items = 0;
    view.selected = -1;
    [view reloadData];
    return NG_SUCCESS;
}

void ng_platform_sidebar_list_invalidate(NGHandle sidebar) {
    if (sidebar) [(__bridge UIView*)sidebar setNeedsDisplay];
}
