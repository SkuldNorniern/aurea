#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

@interface AureaComboBoxDelegate : NSObject <UIPickerViewDataSource, UIPickerViewDelegate>
@property (nonatomic, strong) NSMutableArray<NSString*>* items;
@end

@implementation AureaComboBoxDelegate

- (instancetype)init {
    self = [super init];
    if (self) {
        _items = [[NSMutableArray alloc] init];
    }
    return self;
}

- (NSInteger)numberOfComponentsInPickerView:(UIPickerView*)pickerView {
    return 1;
}

- (NSInteger)pickerView:(UIPickerView*)pickerView numberOfRowsInComponent:(NSInteger)component {
    return [self.items count];
}

- (NSString*)pickerView:(UIPickerView*)pickerView titleForRow:(NSInteger)row forComponent:(NSInteger)component {
    if (row >= 0 && row < [self.items count]) {
        return [self.items objectAtIndex:row];
    }
    return @"";
}

@end

NGHandle ng_ios_create_combo_box(void) {
    UIPickerView* pickerView = [[UIPickerView alloc] init];
    AureaComboBoxDelegate* delegate = [[AureaComboBoxDelegate alloc] init];
    [pickerView setDataSource:delegate];
    [pickerView setDelegate:delegate];
    objc_setAssociatedObject(pickerView, @"delegate", delegate, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    [pickerView setTranslatesAutoresizingMaskIntoConstraints:NO];
    
    return (__bridge_retained void*)pickerView;
}

int ng_ios_combo_box_add_item(NGHandle combo_box, const char* item) {
    if (!combo_box || !item) return NG_ERROR_INVALID_PARAMETER;
    
    UIPickerView* pickerView = (__bridge UIPickerView*)combo_box;
    AureaComboBoxDelegate* delegate = objc_getAssociatedObject(pickerView, @"delegate");
    
    if (!delegate) return NG_ERROR_INVALID_HANDLE;
    
    NSString* nsItem = [NSString stringWithUTF8String:item];
    [delegate.items addObject:nsItem];
    [pickerView reloadAllComponents];
    
    return NG_SUCCESS;
}

int ng_ios_combo_box_set_selected(NGHandle combo_box, int index) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    UIPickerView* pickerView = (__bridge UIPickerView*)combo_box;
    AureaComboBoxDelegate* delegate = objc_getAssociatedObject(pickerView, @"delegate");
    
    if (!delegate) return NG_ERROR_INVALID_HANDLE;
    
    if (index < 0 || index >= [delegate.items count]) {
        return NG_ERROR_INVALID_PARAMETER;
    }
    
    [pickerView selectRow:index inComponent:0 animated:NO];
    return NG_SUCCESS;
}

int ng_ios_combo_box_get_selected(NGHandle combo_box) {
    if (!combo_box) return -1;
    
    UIPickerView* pickerView = (__bridge UIPickerView*)combo_box;
    return (int)[pickerView selectedRowInComponent:0];
}

int ng_ios_combo_box_clear(NGHandle combo_box) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    UIPickerView* pickerView = (__bridge UIPickerView*)combo_box;
    AureaComboBoxDelegate* delegate = objc_getAssociatedObject(pickerView, @"delegate");
    
    if (!delegate) return NG_ERROR_INVALID_HANDLE;
    
    [delegate.items removeAllObjects];
    [pickerView reloadAllComponents];
    
    return NG_SUCCESS;
}

int ng_ios_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    if (!combo_box) return NG_ERROR_INVALID_HANDLE;
    
    UIPickerView* pickerView = (__bridge UIPickerView*)combo_box;
    [pickerView setUserInteractionEnabled:enabled ? YES : NO];
    return NG_SUCCESS;
}

