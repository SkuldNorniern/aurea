#import "window.h"
#import "app_delegate.h"
#import "view_controller.h"
#import "../../common/errors.h"
#import <UIKit/UIKit.h>

static UIWindow* g_mainWindow = nil;
static AureaViewController* g_rootViewController = nil;

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
        }
    });
    
    // Return the window handle (may be NULL if not yet initialized)
    return (__bridge_retained void*)delegate.window;
}

void ng_ios_destroy_window_impl(NGHandle handle) {
    // iOS windows are managed by the app lifecycle
    // This is mostly a no-op
    if (handle) {
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

