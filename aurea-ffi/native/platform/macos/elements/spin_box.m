#import "../elements.h"
#import "../utils.h"
#import "../../../common/errors.h"
#import "../../../common/rust_callbacks.h"
#import <Cocoa/Cocoa.h>

@interface NGSpinBox : NSStackView <NSTextFieldDelegate>
@property(nonatomic, strong) NSTextField* field;
@property(nonatomic, strong) NSStepper* stepper;
@property(nonatomic) unsigned int changeId;
@end

@implementation NGSpinBox
- (void)show:(double)value {
    double clamped = MAX(self.stepper.minValue, MIN(self.stepper.maxValue, value));
    self.stepper.doubleValue = clamped;
    self.field.stringValue = [NSString stringWithFormat:@"%g", clamped];
}

- (void)changed {
    if (self.changeId) ng_invoke_custom_callback(self.changeId);
}

- (void)stepped:(id)sender {
    (void)sender;
    [self show:self.stepper.doubleValue];
    [self changed];
}

- (void)controlTextDidChange:(NSNotification*)note {
    (void)note;
    self.stepper.doubleValue = self.field.doubleValue;
    [self changed];
}

- (void)controlTextDidEndEditing:(NSNotification*)note {
    (void)note;
    [self show:self.field.doubleValue];
}
@end

NGHandle ng_macos_create_spin_box(double min, double max, double step, double value) {
    @autoreleasepool {
        NGSpinBox* box = [[NGSpinBox alloc] init];
        box.orientation = NSUserInterfaceLayoutOrientationHorizontal;
        box.spacing = 4.0;
        box.translatesAutoresizingMaskIntoConstraints = NO;

        NSStepper* stepper = [[NSStepper alloc] init];
        stepper.minValue = min;
        stepper.maxValue = max < min ? min : max;
        stepper.increment = step > 0.0 ? step : 1.0;
        stepper.valueWraps = NO;
        stepper.target = box;
        stepper.action = @selector(stepped:);
        box.stepper = stepper;

        NSTextField* field = [NSTextField textFieldWithString:@""];
        field.delegate = box;
        box.field = field;

        [box addArrangedSubview:field];
        [box addArrangedSubview:stepper];
        [box show:value];
        return (__bridge_retained void*)box;
    }
}

double ng_macos_spin_box_get_value(NGHandle spin_box) {
    if (!spin_box) return 0.0;
    NGSpinBox* box = (__bridge NGSpinBox*)spin_box;
    return MAX(box.stepper.minValue, MIN(box.stepper.maxValue, box.field.doubleValue));
}

int ng_macos_spin_box_set_value(NGHandle spin_box, double value) {
    if (!spin_box) return NG_ERROR_INVALID_HANDLE;
    [(__bridge NGSpinBox*)spin_box show:value];
    return NG_SUCCESS;
}

int ng_macos_spin_box_watch_change(NGHandle element, unsigned int id) {
    NSView* view = (__bridge NSView*)element;
    if (![view isKindOfClass:[NGSpinBox class]]) return 0;
    ((NGSpinBox*)view).changeId = id;
    return 1;
}
