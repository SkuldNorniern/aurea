#import "app_delegate.h"
#import "view_controller.h"
#import "ios.h"
#import "window.h"
#import "../../common/rust_callbacks.h"

static void* g_mainWindowHandle = NULL;
static ScaleFactorCallback g_scaleFactorCallback = NULL;
static BOOL g_lifecycleCallbackEnabled = NO;

@implementation AureaAppDelegate

- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    // Initialize the Aurea platform
    ng_ios_init();
    
    // Create the window
    self.window = [[UIWindow alloc] initWithFrame:[[UIScreen mainScreen] bounds]];
    g_mainWindowHandle = (__bridge void*)self.window;
    
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

- (void)applicationDidEnterBackground:(UIApplication *)application {
    // iOS lifecycle: app entered background
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 0); // ApplicationDidEnterBackground = 0
    }
}

- (void)applicationWillEnterForeground:(UIApplication *)application {
    // iOS lifecycle: app will enter foreground
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 1); // ApplicationWillEnterForeground = 1
    }
}

- (void)applicationDidReceiveMemoryWarning:(UIApplication *)application {
    // iOS lifecycle: memory warning
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 8); // MemoryWarning = 8
    }
}

- (void)applicationWillTerminate:(UIApplication *)application {
    // iOS lifecycle: app will terminate
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 4); // ApplicationDestroyed = 4
    }
}

@end

// Helper functions for scale factor and lifecycle callbacks
void ng_ios_set_main_window_handle(void* handle) {
    g_mainWindowHandle = handle;
}

void ng_ios_set_scale_factor_callback_global(ScaleFactorCallback callback) {
    g_scaleFactorCallback = callback;
}

void ng_ios_set_lifecycle_callback_enabled(BOOL enabled) {
    g_lifecycleCallbackEnabled = enabled;
}
