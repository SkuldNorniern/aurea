#import "view_controller.h"

@implementation AureaViewController

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor whiteColor];
}

- (void)setContentView:(UIView*)view {
    // Remove existing subviews
    for (UIView* subview in [self.view.subviews copy]) {
        [subview removeFromSuperview];
    }
    
    // Add new content view
    if (view) {
        view.translatesAutoresizingMaskIntoConstraints = NO;
        [self.view addSubview:view];
        
        // Set up constraints to fill the view
        [NSLayoutConstraint activateConstraints:@[
            [view.topAnchor constraintEqualToAnchor:self.view.topAnchor],
            [view.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
            [view.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
            [view.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor]
        ]];
    }
}

@end




