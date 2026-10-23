#import "window.h"
#import "app_delegate.h"
#import "view_controller.h"
#import "../../common/errors.h"
#import "../../common/rust_callbacks.h"
#import <UIKit/UIKit.h>

extern void ng_ios_set_main_window_handle(void* handle);
extern void ng_ios_set_scale_factor_callback_global(ScaleFactorCallback callback);
extern void ng_ios_set_lifecycle_callback_enabled(BOOL enabled);

static UIWindow* g_mainWindow = nil;
static AureaViewController* g_rootViewController = nil;
static NSMutableDictionary* g_windowCallbacks = nil;

NGHandle ng_ios_create_window_impl(const char* title, int width, int height) {
    (void)width;
    (void)height;
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
    
    NSString* nsTitle = title ? [NSString stringWithUTF8String:title] : nil;

    dispatch_async(dispatch_get_main_queue(), ^{
        g_mainWindow = delegate.window;
        if (g_mainWindow) {
            g_rootViewController = (AureaViewController*)g_mainWindow.rootViewController;
            
            if (nsTitle && g_rootViewController) {
                g_rootViewController.title = nsTitle;
            }
            
            ng_ios_set_main_window_handle((__bridge void*)g_mainWindow);
        }
    });
    
    return (__bridge_retained void*)delegate.window;
}

void ng_ios_destroy_window_impl(NGHandle handle) {
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
    
    if (!g_windowCallbacks) {
        g_windowCallbacks = [[NSMutableDictionary alloc] init];
    }
    
    UIWindow* uiWindow = (__bridge UIWindow*)window;
    NSValue* windowValue = [NSValue valueWithPointer:(__bridge const void*)uiWindow];
    
    NSValue* callbackValue = [NSValue valueWithPointer:callback];
    [g_windowCallbacks setObject:callbackValue forKey:windowValue];
    
    ng_ios_set_scale_factor_callback_global(callback);
    
    [[NSNotificationCenter defaultCenter] addObserverForName:UIScreenDidConnectNotification
                                                      object:nil
                                                       queue:[NSOperationQueue mainQueue]
                                                  usingBlock:^(NSNotification* note) {
        (void)note;
        if (callback && window) {
            float scale = ng_ios_get_scale_factor(window);
            callback(window, scale);
        }
    }];
}

void ng_ios_window_set_lifecycle_callback(NGHandle window) {
    if (!window) return;
    
    ng_ios_set_lifecycle_callback_enabled(YES);
}

void ng_ios_window_set_title(NGHandle window, const char* title) {
    if (!window) return;
    UIWindow* uiWindow = (__bridge UIWindow*)window;
    NSString* nsTitle = title ? [NSString stringWithUTF8String:title] : @"";
    dispatch_async(dispatch_get_main_queue(), ^{
        uiWindow.rootViewController.title = nsTitle;
    });
}

// An iOS window always fills its screen; the app can't resize it.
void ng_ios_window_set_size(NGHandle window, int width, int height) {
    (void)window; (void)width; (void)height;
}

void ng_ios_window_get_size(NGHandle window, int* width, int* height) {
    CGSize size = window ? ((__bridge UIWindow*)window).bounds.size : CGSizeZero;
    if (width) *width = (int)size.width;
    if (height) *height = (int)size.height;
}

// iOS apps don't close their own windows.
void ng_ios_window_request_close(NGHandle window) {
    (void)window;
}

int ng_ios_window_is_focused(NGHandle window) {
    return window && ((__bridge UIWindow*)window).isKeyWindow ? 1 : 0;
}
