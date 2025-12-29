#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import <Cocoa/Cocoa.h>

NGHandle ng_macos_create_combo_box(void) {
    NSPopUpButton* comboBox = [[NSPopUpButton alloc] init];
    [comboBox setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)comboBox;
}

int ng_macos_combo_box_add_item(NGHandle combo_box, const char* item) {
    if (!combo_box || !item) return NG_ERROR_INVALID_PARAMETER;
    
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    NSString* nsItem = ng_macos_to_nsstring(item);
    [comboBox addItemWithTitle:nsItem];
    
    return NG_SUCCESS;
}

int ng_macos_combo_box_set_selected(NGHandle combo_box, int index) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    NSInteger itemCount = [comboBox numberOfItems];
    
    if (index < 0 || index >= itemCount) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    
    [comboBox selectItemAtIndex:index];
    return NG_SUCCESS;
}

int ng_macos_combo_box_get_selected(NGHandle combo_box) {
    if (!combo_box) return -1;
    
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    return (int)[comboBox indexOfSelectedItem];
}

int ng_macos_combo_box_clear(NGHandle combo_box) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    [comboBox removeAllItems];
    
    return NG_SUCCESS;
}

int ng_macos_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    [comboBox setEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

void ng_macos_combo_box_invalidate(NGHandle combo_box) {
    if (!combo_box) return;
    NSPopUpButton* comboBox = (__bridge NSPopUpButton*)combo_box;
    [comboBox setNeedsDisplay:YES];
}

