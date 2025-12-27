#import "window.h"
#import "app_delegate.h"
#import "view_controller.h"
#import "../../common/errors.h"
#import <UIKit/UIKit.h>

// Forward declaration for lifecycle callback
extern void ng_invoke_lifecycle_callback(void* window, unsigned int event_id);
extern void ng_ios_set_main_window_handle(void* handle);
extern void ng_ios_set_scale_factor_callback_global(ScaleFactorCallback callback);
extern void ng_ios_set_lifecycle_callback_enabled(BOOL enabled);

static UIWindow* g_mainWindow = nil;
static AureaViewController* g_rootViewController = nil;
static NSMutableDictionary* g_windowCallbacks = nil;

NGHandle ng_ios_create_window_impl(const char* title, int width, int height) {
    // iOS uses full screen, so width/height are ignored
    (void)width;
    (void)height;
    // On iOS, the window is created in the app delegate
    // We just need to get a reference to it
    UIApplication* app = [UIApplication sharedApplication];
    if (!app) {
        return NULL;
    }
    
    AureaAppDelegate* delegate = (AureaAppDelegate*)[app delegate];
    if (!delegate) {
        return NULL;
    }
    
    if (!g_windowCallbacks) {
        g_windowCallbacks = [[NSMutableDictionary alloc] init];
    }
    
    // Wait a bit for the app delegate to finish initialization
    // In a real app, this would be called after didFinishLaunchingWithOptions
    dispatch_async(dispatch_get_main_queue(), ^{
        g_mainWindow = delegate.window;
        if (g_mainWindow) {
            g_rootViewController = (AureaViewController*)g_mainWindow.rootViewController;
            
            // Set window title (iOS doesn't have window titles, but we can set the view controller title)
            if (title && g_rootViewController) {
                g_rootViewController.title = [NSString stringWithUTF8String:title];
            }
            
            // Set main window handle for lifecycle callbacks
            ng_ios_set_main_window_handle((__bridge void*)g_mainWindow);
        }
    });
    
    // Return the window handle (may be NULL if not yet initialized)
    return (__bridge_retained void*)delegate.window;
}

void ng_ios_destroy_window_impl(NGHandle handle) {
    // iOS windows are managed by the app lifecycle
    // This is mostly a no-op
    if (handle) {
        if (g_windowCallbacks) {
            NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)(__bridge UIWindow*)handle];
            [g_windowCallbacks removeObjectForKey:windowValue];
        }
        g_mainWindow = nil;
        g_rootViewController = nil;
    }
}

int ng_ios_set_window_content(NGHandle window_handle, NGHandle content_handle) {
    if (!window_handle || !content_handle) return NG_ERROR_INVALID_HANDLE;
    
    UIWindow* window = (__bridge UIWindow*)window_handle;
    UIView* content = (__bridge UIView*)content_handle;
    
    AureaViewController* vc = (AureaViewController*)window.rootViewController;
    if (vc) {
        [vc setContentView:content];
        return NG_SUCCESS;
    }
    
    return NG_ERROR_INVALID_HANDLE;
}

float ng_ios_get_scale_factor(NGHandle window) {
    if (!window) return 1.0f;
    UIWindow* uiWindow = (__bridge UIWindow*)window;
    UIScreen* screen = [uiWindow screen];
    if (screen) {
        return (float)[screen nativeScale];
    }
    return 1.0f;
}

void ng_ios_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    if (!window) return;
    
    // Store callback for this window
    if (!g_windowCallbacks) {
        g_windowCallbacks = [[NSMutableDictionary alloc] init];
    }
    
    UIWindow* uiWindow = (__bridge UIWindow*)window;
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)uiWindow];
    
    // Store callback in dictionary (wrapped in NSValue)
    NSValue* callbackValue = [NSValue valueWithPointer:callback];
    [g_windowCallbacks setObject:callbackValue forKey:windowValue];
    
    // Also set global callback for app delegate
    ng_ios_set_scale_factor_callback_global(callback);
    
    // Observe screen scale changes
    [[NSNotificationCenter defaultCenter] addObserverForName:UIScreenDidConnectNotification
                                                      object:nil
                                                       queue:[NSOperationQueue mainQueue]
                                                  usingBlock:^(NSNotification* note) {
        if (callback && window) {
            float scale = ng_ios_get_scale_factor(window);
            callback(window, scale);
        }
    }];
}

void ng_ios_window_set_lifecycle_callback(NGHandle window) {
    if (!window) return;
    
    // Enable lifecycle callbacks globally (iOS has single window)
    ng_ios_set_lifecycle_callback_enabled(YES);
}

