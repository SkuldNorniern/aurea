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
    ng_ios_init();
    
    self.window = [[UIWindow alloc] initWithFrame:[[UIScreen mainScreen] bounds]];
    g_mainWindowHandle = (__bridge void*)self.window;
    
    AureaViewController* rootViewController = [[AureaViewController alloc] init];
    
    self.window.rootViewController = rootViewController;
    
    [self.window makeKeyAndVisible];
    
    
    return YES;
}

- (void)applicationDidEnterBackground:(UIApplication *)application {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 0);
    }
}

- (void)applicationWillEnterForeground:(UIApplication *)application {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 1);
    }
}

- (void)applicationDidReceiveMemoryWarning:(UIApplication *)application {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 8);
    }
}

- (void)applicationWillTerminate:(UIApplication *)application {
    if (g_lifecycleCallbackEnabled && g_mainWindowHandle) {
        ng_invoke_lifecycle_callback(g_mainWindowHandle, 4);
    }
}

@end

void ng_ios_set_main_window_handle(void* handle) {
    g_mainWindowHandle = handle;
}

void ng_ios_set_scale_factor_callback_global(ScaleFactorCallback callback) {
    g_scaleFactorCallback = callback;
}

void ng_ios_set_lifecycle_callback_enabled(BOOL enabled) {
    g_lifecycleCallbackEnabled = enabled;
}
