#import "ios.h"
#import "window.h"
#import "elements.h"
#import "../../common/errors.h"
#import "../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

int ng_ios_init(void) {
    return NG_SUCCESS;
}

void ng_ios_cleanup(void) {
}

NGHandle ng_ios_create_window(const char* title, int width, int height) {
    return ng_ios_create_window_impl(title, width, height);
}

void ng_ios_destroy_window(NGHandle handle) {
    ng_ios_destroy_window_impl(handle);
}

/* The app's menus: a tree the Rust side builds of submenus, items and separators. UIKit shows it in
   the menu bar, from `ng_ios_build_menu`, and its shortcuts come from `ng_ios_key_commands`. */
@interface AureaMenuNode : NSObject
@property (copy) NSString* title;
/* The key command's input and modifiers, none when the item has no shortcut. */
@property (copy) NSString* input;
@property (assign) UIKeyModifierFlags flags;
@property (assign) uint32_t itemId;
@property (assign) BOOL enabled;
@property (assign) BOOL checked;
@property (assign) BOOL separator;
/* The contents of a submenu or the menu bar; nil for an item. */
@property (strong) NSMutableArray<AureaMenuNode*>* children;
@end

@implementation AureaMenuNode

- (instancetype)init {
    self = [super init];
    if (self) _enabled = YES;
    return self;
}

@end

/* The menu bar the window attached, and the item each key command runs, by input and modifiers. */
static AureaMenuNode* g_menu;
static NSMutableDictionary<NSString*, NSNumber*>* g_keys;

static void ng_ios_menus_changed(void) {
    if (@available(iOS 13.0, *)) [UIMenuSystem.mainSystem setNeedsRebuild];
}

/* The item with `itemId` under `menu`, and the menu holding it through `owner`. */
static AureaMenuNode* ng_ios_find(AureaMenuNode* menu, uint32_t itemId, AureaMenuNode** owner) {
    for (AureaMenuNode* node in menu.children) {
        if (node.children) {
            AureaMenuNode* found = ng_ios_find(node, itemId, owner);
            if (found) return found;
        } else if (!node.separator && node.itemId == itemId) {
            if (owner) *owner = menu;
            return node;
        }
    }
    return nil;
}

static void ng_ios_run_item(uint32_t itemId) {
    AureaMenuNode* item = ng_ios_find(g_menu, itemId, NULL);
    if (item && item.enabled) ng_invoke_menu_callback((unsigned int)itemId);
}

/* Reads a title such as "Save\tCmd+S": the title, and for the shortcut after the tab its input and
   modifiers. A bare key with no modifier is left out, as it is typed text. */
static void ng_ios_read_label(NSString* label, AureaMenuNode* node) {
    NSRange tab = [label rangeOfString:@"\t"];
    node.title = tab.location == NSNotFound ? label : [label substringToIndex:tab.location];
    node.input = nil;
    node.flags = 0;
    if (tab.location == NSNotFound) return;

    NSMutableArray<NSString*>* tokens = [[[label substringFromIndex:tab.location + 1] componentsSeparatedByString:@"+"] mutableCopy];
    NSString* key = tokens.lastObject;
    if (key.length) {
        [tokens removeLastObject];
    } else {
        // "Ctrl++" is the plus key: two empty tokens after the separator.
        if (tokens.count < 2) return;
        key = @"+";
        [tokens removeLastObject];
        [tokens removeLastObject];
    }

    UIKeyModifierFlags flags = 0;
    for (NSString* token in tokens) {
        NSString* name = token.lowercaseString;
        if ([name isEqualToString:@"ctrl"] || [name isEqualToString:@"control"]) {
            flags |= UIKeyModifierControl;
        } else if ([name isEqualToString:@"cmd"] || [name isEqualToString:@"command"] || [name isEqualToString:@"meta"]) {
            flags |= UIKeyModifierCommand;
        } else if ([name isEqualToString:@"alt"] || [name isEqualToString:@"option"]) {
            flags |= UIKeyModifierAlternate;
        } else if ([name isEqualToString:@"shift"]) {
            flags |= UIKeyModifierShift;
        }
    }

    NSString* input = nil;
    NSString* name = key.lowercaseString;
    if (name.length == 1) {
        input = name;
    } else if ([name isEqualToString:@"enter"] || [name isEqualToString:@"return"]) {
        input = @"\r";
    } else if ([name isEqualToString:@"tab"]) {
        input = @"\t";
    } else if ([name isEqualToString:@"space"]) {
        input = @" ";
    } else if ([name isEqualToString:@"escape"] || [name isEqualToString:@"esc"]) {
        input = UIKeyInputEscape;
    } else if ([name isEqualToString:@"up"]) {
        input = UIKeyInputUpArrow;
    } else if ([name isEqualToString:@"down"]) {
        input = UIKeyInputDownArrow;
    } else if ([name isEqualToString:@"left"]) {
        input = UIKeyInputLeftArrow;
    } else if ([name isEqualToString:@"right"]) {
        input = UIKeyInputRightArrow;
    } else if ([name isEqualToString:@"pageup"]) {
        input = UIKeyInputPageUp;
    } else if ([name isEqualToString:@"pagedown"]) {
        input = UIKeyInputPageDown;
    } else if (@available(iOS 13.4, *)) {
        if ([name isEqualToString:@"home"]) {
            input = UIKeyInputHome;
        } else if ([name isEqualToString:@"end"]) {
            input = UIKeyInputEnd;
        } else if (name.length >= 2 && name.length <= 3 && [name hasPrefix:@"f"]) {
            NSString* functions[] = {
                UIKeyInputF1, UIKeyInputF2, UIKeyInputF3, UIKeyInputF4, UIKeyInputF5, UIKeyInputF6,
                UIKeyInputF7, UIKeyInputF8, UIKeyInputF9, UIKeyInputF10, UIKeyInputF11, UIKeyInputF12,
            };
            NSInteger number = [[name substringFromIndex:1] integerValue];
            if (number >= 1 && number <= 12) input = functions[number - 1];
        }
    }
    if (!input || (input.length == 1 && !flags)) return;
    node.input = input;
    node.flags = flags;
}

/* The UIKit elements for `nodes`: each run of items between separators is an inline section. */
static void ng_ios_section(NSMutableArray<UIMenuElement*>* out, NSMutableArray<UIMenuElement*>* section) {
    if (section.count) {
        [out addObject:[UIMenu menuWithTitle:@"" image:nil identifier:nil options:UIMenuOptionsDisplayInline children:[section copy]]];
    }
    [section removeAllObjects];
}

static UIAction* ng_ios_action(AureaMenuNode* node) {
    uint32_t itemId = node.itemId;
    UIAction* action = [UIAction actionWithTitle:node.title
                                           image:nil
                                      identifier:nil
                                         handler:^(UIAction* sender) {
                                             (void)sender;
                                             ng_ios_run_item(itemId);
                                         }];
    action.attributes = node.enabled ? 0 : UIMenuElementAttributesDisabled;
    action.state = node.checked ? UIMenuElementStateOn : UIMenuElementStateOff;
    return action;
}

static NSArray<UIMenuElement*>* ng_ios_elements(NSArray<AureaMenuNode*>* nodes) {
    NSMutableArray<UIMenuElement*>* out = [NSMutableArray array];
    NSMutableArray<UIMenuElement*>* section = [NSMutableArray array];
    for (AureaMenuNode* node in nodes) {
        if (node.separator) {
            ng_ios_section(out, section);
        } else if (node.children) {
            [section addObject:[UIMenu menuWithTitle:node.title children:ng_ios_elements(node.children)]];
        } else {
            [section addObject:ng_ios_action(node)];
        }
    }
    ng_ios_section(out, section);
    return out;
}

static void ng_ios_collect_keys(AureaMenuNode* menu, NSMutableArray<UIKeyCommand*>* keys) {
    for (AureaMenuNode* node in menu.children) {
        if (node.children) {
            ng_ios_collect_keys(node, keys);
        } else if (node.input) {
            UIKeyCommand* key = [UIKeyCommand keyCommandWithInput:node.input
                                                    modifierFlags:node.flags
                                                           action:@selector(keyCommand:)];
            key.discoverabilityTitle = node.title;
            [keys addObject:key];
            g_keys[[NSString stringWithFormat:@"%@|%ld", node.input, (long)node.flags]] = @(node.itemId);
        }
    }
}

NSArray<UIKeyCommand*>* ng_ios_key_commands(void) {
    NSMutableArray<UIKeyCommand*>* keys = [NSMutableArray array];
    g_keys = [NSMutableDictionary dictionary];
    if (g_menu) ng_ios_collect_keys(g_menu, keys);
    return keys;
}

void ng_ios_key_command(UIKeyCommand* key) {
    NSNumber* itemId = g_keys[[NSString stringWithFormat:@"%@|%ld", key.input, (long)key.modifierFlags]];
    if (itemId) ng_ios_run_item(itemId.unsignedIntValue);
}

void ng_ios_build_menu(id<UIMenuBuilder> builder) {
    // UIKit also builds context menus here; only the menu bar is the app's.
    if (!g_menu.children.count || builder.system != UIMenuSystem.mainSystem) return;
    // The app's menus replace UIKit's, which would otherwise stay beside them.
    for (NSString* identifier in @[UIMenuFile, UIMenuEdit, UIMenuView, UIMenuWindow, UIMenuHelp]) {
        [builder removeMenuForIdentifier:identifier];
    }
    // Each menu goes after the app menu; in reverse, so they come out in order.
    for (AureaMenuNode* node in g_menu.children.reverseObjectEnumerator) {
        if (!node.children) continue;
        UIMenu* menu = [UIMenu menuWithTitle:node.title children:ng_ios_elements(node.children)];
        [builder insertSiblingMenu:menu afterMenuForIdentifier:UIMenuApplication];
    }
}

NGMenuHandle ng_ios_create_menu(void) {
    AureaMenuNode* root = [[AureaMenuNode alloc] init];
    root.children = [NSMutableArray array];
    return (__bridge_retained void*)root;
}

void ng_ios_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    AureaMenuNode* root = (__bridge_transfer AureaMenuNode*)handle;
    if (root == g_menu) {
        g_menu = nil;
        ng_ios_menus_changed();
    }
}

int ng_ios_attach_menu(NGMenuHandle handle) {
    if (!handle) return NG_ERROR_INVALID_HANDLE;
    g_menu = (__bridge AureaMenuNode*)handle;
    ng_ios_menus_changed();
    return NG_SUCCESS;
}

static AureaMenuNode* ng_ios_add(NGMenuHandle handle, const char* title, unsigned int itemId) {
    AureaMenuNode* parent = (__bridge AureaMenuNode*)handle;
    if (!parent.children || !title) return nil;
    AureaMenuNode* node = [[AureaMenuNode alloc] init];
    node.itemId = itemId;
    [parent.children addObject:node];
    ng_ios_read_label([NSString stringWithUTF8String:title], node);
    ng_ios_menus_changed();
    return node;
}

int ng_ios_add_menu_item(NGMenuHandle menu, const char* title, unsigned int itemId) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    return ng_ios_add(menu, title, itemId) ? NG_SUCCESS : NG_ERROR_INVALID_PARAMETER;
}

int ng_ios_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int itemId, int checked) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    AureaMenuNode* node = ng_ios_add(menu, title, itemId);
    if (!node) return NG_ERROR_INVALID_PARAMETER;
    node.checked = checked != 0;
    return NG_SUCCESS;
}

int ng_ios_add_menu_separator(NGMenuHandle menu) {
    AureaMenuNode* parent = (__bridge AureaMenuNode*)menu;
    if (!parent.children) return NG_ERROR_INVALID_HANDLE;
    AureaMenuNode* node = [[AureaMenuNode alloc] init];
    node.separator = YES;
    [parent.children addObject:node];
    ng_ios_menus_changed();
    return NG_SUCCESS;
}

/* The submenu is owned by its parent, which keeps it until it is cleared or the menu is destroyed. */
NGMenuHandle ng_ios_create_submenu(NGMenuHandle parent, const char* title) {
    AureaMenuNode* owner = (__bridge AureaMenuNode*)parent;
    if (!owner.children || !title) return NULL;
    AureaMenuNode* node = [[AureaMenuNode alloc] init];
    node.title = [NSString stringWithUTF8String:title];
    node.children = [NSMutableArray array];
    [owner.children addObject:node];
    ng_ios_menus_changed();
    return (__bridge void*)node;
}

int ng_ios_menu_clear(NGMenuHandle menu) {
    AureaMenuNode* node = (__bridge AureaMenuNode*)menu;
    if (!node.children) return NG_ERROR_INVALID_HANDLE;
    [node.children removeAllObjects];
    ng_ios_menus_changed();
    return NG_SUCCESS;
}

/* The item `itemId` in `menu`, changed by `change`, then the menus are rebuilt. */
static int ng_ios_change(NGMenuHandle menu, unsigned int itemId, void (^change)(AureaMenuNode*)) {
    AureaMenuNode* item = ng_ios_find((__bridge AureaMenuNode*)menu, itemId, NULL);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    change(item);
    ng_ios_menus_changed();
    return NG_SUCCESS;
}

int ng_ios_menu_item_set_enabled(NGMenuHandle menu, unsigned int itemId, int enabled) {
    return ng_ios_change(menu, itemId, ^(AureaMenuNode* item) { item.enabled = enabled != 0; });
}

int ng_ios_menu_item_set_checked(NGMenuHandle menu, unsigned int itemId, int checked) {
    return ng_ios_change(menu, itemId, ^(AureaMenuNode* item) { item.checked = checked != 0; });
}

int ng_ios_menu_item_set_title(NGMenuHandle menu, unsigned int itemId, const char* title) {
    if (!title) return NG_ERROR_INVALID_PARAMETER;
    NSString* label = [NSString stringWithUTF8String:title];
    return ng_ios_change(menu, itemId, ^(AureaMenuNode* item) { ng_ios_read_label(label, item); });
}

int ng_ios_menu_item_remove(NGMenuHandle menu, unsigned int itemId) {
    AureaMenuNode* owner = nil;
    AureaMenuNode* item = ng_ios_find((__bridge AureaMenuNode*)menu, itemId, &owner);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    [owner.children removeObjectIdenticalTo:item];
    ng_ios_menus_changed();
    return NG_SUCCESS;
}

NGHandle ng_ios_create_button(const char* title, unsigned int id) {
    return ng_ios_create_button_impl(title, id);
}

NGHandle ng_ios_create_label(const char* text) {
    return ng_ios_create_label_impl(text);
}

NGHandle ng_ios_create_canvas(int width, int height) {
    return ng_ios_create_canvas_impl(width, height);
}

void ng_platform_button_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_label_invalidate(NGHandle handle) {
    (void)handle;
}

NGHandle ng_platform_create_scroll_view(void) {
    return NULL;
}

int ng_platform_scroll_view_set_content(NGHandle scroll, NGHandle element) {
    (void)scroll;
    (void)element;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_scroll_view_set_scrollbar(NGHandle scroll, int policy) {
    (void)scroll;
    (void)policy;
    return NG_ERROR_UNSUPPORTED;
}

static char kDestroyIdKey;
static char kChangeIdKey;
static char kMinWidthKey;
static char kMinHeightKey;

@interface NGIOSChangeTarget : NSObject
- (void)changed:(id)sender;
@end

@implementation NGIOSChangeTarget
- (void)changed:(id)sender {
    NSNumber* changeId = objc_getAssociatedObject(sender, &kChangeIdKey);
    if (changeId) ng_invoke_custom_callback(changeId.unsignedIntValue);
}
@end

static void ng_ios_enable_view(UIView* view, BOOL enabled) {
    if ([view isKindOfClass:[UIControl class]]) ((UIControl*)view).enabled = enabled;
    else if ([view isKindOfClass:[UILabel class]]) ((UILabel*)view).enabled = enabled;
    for (UIView* sub in view.subviews) ng_ios_enable_view(sub, enabled);
}

void ng_ios_notify_destroyed(UIView* view) {
    if (!view) return;
    for (UIView* sub in [view.subviews copy]) ng_ios_notify_destroyed(sub);
    NSNumber* watch = objc_getAssociatedObject(view, &kDestroyIdKey);
    if (watch) {
        objc_setAssociatedObject(view, &kDestroyIdKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        ng_invoke_custom_callback(watch.unsignedIntValue);
    }
}

static void ng_ios_min_constraint(UIView* view, void* key, NSLayoutDimension* dimension, int value) {
    NSLayoutConstraint* old = objc_getAssociatedObject(view, key);
    if (old) old.active = NO;
    NSLayoutConstraint* min = nil;
    if (value > 0) {
        min = [dimension constraintGreaterThanOrEqualToConstant:value];
        min.priority = UILayoutPriorityRequired - 1;
        min.active = YES;
    }
    objc_setAssociatedObject(view, key, min, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

int ng_platform_label_set_style(NGHandle label, int style) {
    if (!label) return NG_ERROR_INVALID_HANDLE;
    if (style < NG_TEXT_STYLE_BODY || style > NG_TEXT_STYLE_HEADING) return NG_ERROR_INVALID_PARAMETER;
    CGFloat size = UIFont.systemFontSize;
    UIFont* font = style == NG_TEXT_STYLE_STRONG ? [UIFont systemFontOfSize:size weight:UIFontWeightSemibold]
                 : style == NG_TEXT_STYLE_SMALL ? [UIFont systemFontOfSize:UIFont.smallSystemFontSize]
                 : style == NG_TEXT_STYLE_HEADING ? [UIFont systemFontOfSize:size * 1.4 weight:UIFontWeightSemibold]
                                                  : [UIFont systemFontOfSize:size];
    UILabel* view = (__bridge UILabel*)label;
    view.font = font;
    [view invalidateIntrinsicContentSize];
    return NG_SUCCESS;
}

int ng_platform_label_set_text(NGHandle handle, const char* text) {
    if (!handle) return NG_ERROR_INVALID_HANDLE;
    if (!text) return NG_ERROR_INVALID_PARAMETER;
    UILabel* label = (__bridge UILabel*)handle;
    [label setText:[NSString stringWithUTF8String:text]];
    return NG_SUCCESS;
}

void ng_platform_canvas_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_text_editor_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_text_view_invalidate(NGHandle handle) {
    (void)handle;
}

void ng_platform_progress_bar_invalidate(NGHandle handle) {
    (void)handle;
}

int ng_platform_element_set_enabled(NGHandle element, int enabled) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    ng_ios_enable_view((__bridge UIView*)element, enabled ? YES : NO);
    return NG_SUCCESS;
}

int ng_platform_element_watch_destroy(NGHandle element, unsigned int id) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    objc_setAssociatedObject((__bridge UIView*)element, &kDestroyIdKey, @(id), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    return NG_SUCCESS;
}

int ng_platform_text_unit(void) {
    return NG_TEXT_UNIT_UTF16;
}

int ng_platform_text_get_selection(NGHandle text, int* start, int* end) {
    (void)text;
    (void)start;
    (void)end;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_set_selection(NGHandle text, int start, int end) {
    (void)text;
    (void)start;
    (void)end;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_replace(NGHandle text, int start, int end, const char* replacement) {
    (void)text;
    (void)start;
    (void)end;
    (void)replacement;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_undo(NGHandle text, int redo) {
    (void)text;
    (void)redo;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_text_can_undo(NGHandle text, int redo) {
    (void)text;
    (void)redo;
    return 0;
}

int ng_platform_text_set_feature(NGHandle text, int feature, int on) {
    (void)text;
    (void)feature;
    (void)on;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_element_set_change_id(NGHandle element, unsigned int id) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    UIView* view = (__bridge UIView*)element;
    if (![view isKindOfClass:[UIControl class]]) return NG_ERROR_UNSUPPORTED;
    static NGIOSChangeTarget* target = nil;
    if (!target) target = [[NGIOSChangeTarget alloc] init];
    objc_setAssociatedObject(view, &kChangeIdKey, @(id), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    [(UIControl*)view addTarget:target action:@selector(changed:) forControlEvents:UIControlEventValueChanged];
    return NG_SUCCESS;
}

int ng_platform_element_set_visible(NGHandle element, int visible) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    ((__bridge UIView*)element).hidden = visible ? NO : YES;
    return NG_SUCCESS;
}

int ng_platform_element_set_min_size(NGHandle element, int width, int height) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (width < 0 || height < 0) return NG_ERROR_INVALID_PARAMETER;
    UIView* view = (__bridge UIView*)element;
    ng_ios_min_constraint(view, &kMinWidthKey, view.widthAnchor, width);
    ng_ios_min_constraint(view, &kMinHeightKey, view.heightAnchor, height);
    return NG_SUCCESS;
}

int ng_platform_element_set_accessible_name(NGHandle element, const char* name) {
    if (!element) return NG_ERROR_INVALID_HANDLE;
    if (!name) return NG_ERROR_INVALID_PARAMETER;
    ((__bridge UIView*)element).accessibilityLabel = *name ? [NSString stringWithUTF8String:name] : nil;
    return NG_SUCCESS;
}

int ng_platform_box_set_spacing(NGHandle box, int spacing) {
    if (!box) return NG_ERROR_INVALID_HANDLE;
    UIView* view = (__bridge UIView*)box;
    if (![view isKindOfClass:[UIStackView class]]) return NG_ERROR_INVALID_HANDLE;
    if (spacing < 0) return NG_ERROR_INVALID_PARAMETER;
    ((UIStackView*)view).spacing = spacing;
    return NG_SUCCESS;
}

int ng_platform_box_set_padding(NGHandle box, int padding) {
    if (!box) return NG_ERROR_INVALID_HANDLE;
    UIView* view = (__bridge UIView*)box;
    if (![view isKindOfClass:[UIStackView class]]) return NG_ERROR_INVALID_HANDLE;
    if (padding < 0) return NG_ERROR_INVALID_PARAMETER;
    UIStackView* stack = (UIStackView*)view;
    stack.layoutMarginsRelativeArrangement = YES;
    stack.layoutMargins = UIEdgeInsetsMake(padding, padding, padding, padding);
    return NG_SUCCESS;
}

