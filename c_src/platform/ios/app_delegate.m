#import "app_delegate.h"
#import "view_controller.h"
#import "ios.h"

@implementation AureaAppDelegate

- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    // Initialize the Aurea platform
    ng_ios_init();
    
    // Create the window
    self.window = [[UIWindow alloc] initWithFrame:[[UIScreen mainScreen] bounds]];
    
    // Create the root view controller
    AureaViewController* rootViewController = [[AureaViewController alloc] init];
    
    // Set as root view controller
    self.window.rootViewController = rootViewController;
    
    // Make window key and visible
    [self.window makeKeyAndVisible];
    
    // Call Rust code to set up UI (this will be implemented in the Rust example)
    // The Rust code should call Window::new() and set up the UI here
    // For now, this is a placeholder that can be called from Rust via FFI
    
    return YES;
}

@end

