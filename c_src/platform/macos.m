#import "macos.h"
#import "macos/window.h"
#import "macos/menu.h"
#import "../common/errors.h"
#import "macos/elements.h"


#import <Cocoa/Cocoa.h>
#import <CoreFoundation/CoreFoundation.h>
static BOOL app_initialized = FALSE;

@interface AppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation AppDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}
@end

static AppDelegate* app_delegate = nil;

int ng_platform_init(void) {
    if (!app_initialized) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        
        app_delegate = [[AppDelegate alloc] init];
        [NSApp setDelegate:app_delegate];
        
        [NSApp finishLaunching];
        app_initialized = TRUE;
    }
    return NG_SUCCESS;
}

void ng_platform_cleanup(void) {
    if (app_initialized) {
        app_initialized = FALSE;
    }
}

NGHandle ng_platform_create_window(const char* title, int width, int height) {
    return ng_macos_create_window(title, width, height);
}

NGHandle ng_platform_create_window_with_type(const char* title, int width, int height, int window_type) {
    return ng_macos_create_window_with_type(title, width, height, window_type);
}

void ng_platform_destroy_window(NGHandle handle) {
    ng_macos_destroy_window(handle);
}

void ng_platform_window_show(NGHandle window) {
    ng_macos_window_show(window);
}

void ng_platform_window_hide(NGHandle window) {
    ng_macos_window_hide(window);
}

int ng_platform_window_is_visible(NGHandle window) {
    return ng_macos_window_is_visible(window);
}

NGMenuHandle ng_platform_create_menu(void) {
    return ng_macos_create_menu();
}

void ng_platform_destroy_menu(NGMenuHandle handle) {
    ng_macos_destroy_menu(handle);
}

int ng_platform_attach_menu(NGHandle window, NGMenuHandle menu) {
    return ng_macos_attach_menu(window, menu);
}

int ng_platform_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    return ng_macos_add_menu_item(menu, title, id);
}

NGMenuHandle ng_platform_create_submenu(NGMenuHandle parentMenu, const char* title) {
    return ng_macos_create_submenu(parentMenu, title);
}

extern void ng_process_frames(void);

// Timer callback to process frames periodically
static void process_frames_timer(CFRunLoopTimerRef timer, void *info) {
    (void)timer;
    (void)info;
    ng_process_frames();
}

int ng_platform_run(void) {
    // Add a timer to process frames periodically (60fps = ~16ms)
    CFRunLoopTimerRef timer = CFRunLoopTimerCreate(
        kCFAllocatorDefault,
        CFAbsoluteTimeGetCurrent(),
        1.0/60.0, // 60fps
        0,
        0,
        process_frames_timer,
        NULL
    );
    if (timer) {
        CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
    }
    
    [NSApp run];
    
    if (timer) {
        CFRunLoopTimerInvalidate(timer);
        CFRelease(timer);
    }
    
    return NG_SUCCESS;
}

int ng_platform_poll_events(void) {
    @autoreleasepool {
        while (true) {
            NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                              untilDate:[NSDate distantPast]
                                                 inMode:NSDefaultRunLoopMode
                                                dequeue:YES];
            if (event == nil) {
                break;
            }
            [NSApp sendEvent:event];
        }
    }
    return NG_SUCCESS;
}

NGHandle ng_platform_create_button(const char* title, unsigned int id) {
    return ng_macos_create_button(title, id);
}

NGHandle ng_platform_create_label(const char* text) {
    return ng_macos_create_label(text);
}

NGHandle ng_platform_create_box(int is_vertical) {
    return ng_macos_create_box(is_vertical);
}

void ng_platform_box_invalidate(NGHandle box) {
    ng_macos_box_invalidate(box);
}

int ng_platform_box_add(NGHandle box, NGHandle element, float weight) {
    return ng_macos_box_add(box, element, weight);
}

int ng_platform_set_window_content(NGHandle window, NGHandle content) {
    return ng_macos_set_window_content(window, content);
}

NGHandle ng_platform_create_text_editor(unsigned int id) {
    return ng_macos_create_text_editor(id);
}

void ng_platform_text_editor_invalidate(NGHandle text_editor) {
    ng_macos_text_editor_invalidate(text_editor);
}

NGHandle ng_platform_create_text_view(int is_editable, unsigned int id) {
    return ng_macos_create_text_view(is_editable, id);
}

void ng_platform_text_view_invalidate(NGHandle text_view) {
    ng_macos_text_view_invalidate(text_view);
}

int ng_platform_set_text_content(NGHandle text_handle, const char* content) {
    return ng_macos_set_text_content(text_handle, content);
}

char* ng_platform_get_text_content(NGHandle text_handle) {
    return ng_macos_get_text_content(text_handle);
}

void ng_platform_free_text_content(char* content) {
    ng_macos_free_text_content(content);
} 

NGHandle ng_platform_create_canvas(int width, int height) {
    return ng_macos_create_canvas(width, height);
}

void ng_platform_canvas_invalidate(NGHandle canvas) {
    ng_macos_canvas_invalidate(canvas);
}

void ng_platform_canvas_invalidate_rect(NGHandle canvas, float x, float y, float width, float height) {
    ng_macos_canvas_invalidate_rect(canvas, x, y, width, height);
}

void ng_platform_canvas_update_buffer(NGHandle canvas, const unsigned char* buffer, unsigned int size, unsigned int width, unsigned int height) {
    ng_macos_canvas_update_buffer(canvas, buffer, size, width, height);
}

void ng_platform_canvas_get_size(NGHandle canvas, unsigned int* width, unsigned int* height) {
    ng_macos_canvas_get_size(canvas, width, height);
}

NGHandle ng_platform_canvas_get_window(NGHandle canvas) {
    return ng_macos_canvas_get_window(canvas);
}

float ng_platform_get_scale_factor(NGHandle window) {
    return ng_macos_get_scale_factor(window);
}

void ng_platform_window_set_scale_factor_callback(NGHandle window, ScaleFactorCallback callback) {
    ng_macos_window_set_scale_factor_callback(window, callback);
}

void ng_platform_window_set_lifecycle_callback(NGHandle window) {
    ng_macos_window_set_lifecycle_callback(window);
}

void ng_platform_window_set_title(NGHandle window, const char* title) {
    ng_macos_window_set_title(window, title);
}

void ng_platform_window_set_size(NGHandle window, int width, int height) {
    ng_macos_window_set_size(window, width, height);
}

void ng_platform_window_get_size(NGHandle window, int* width, int* height) {
    ng_macos_window_get_size(window, width, height);
}
 
void ng_platform_window_set_position(NGHandle window, int x, int y) {
    ng_macos_window_set_position(window, x, y);
}

void ng_platform_window_get_position(NGHandle window, int* x, int* y) {
    ng_macos_window_get_position(window, x, y);
}

void ng_platform_window_request_close(NGHandle window) {
    ng_macos_window_request_close(window);
}

int ng_platform_window_is_focused(NGHandle window) {
    return ng_macos_window_is_focused(window);
}

NGHandle ng_platform_window_get_content_view(NGHandle window) {
    return ng_macos_window_get_content_view(window);
}

void ng_platform_button_invalidate(NGHandle button) {
    ng_macos_button_invalidate(button);
}

void ng_platform_label_invalidate(NGHandle label) {
    ng_macos_label_invalidate(label);
}

// ImageView functions
NGHandle ng_platform_create_image_view(void) {
    return ng_macos_create_image_view();
}

int ng_platform_image_view_load_from_path(NGHandle image_view, const char* path) {
    return ng_macos_image_view_load_from_path(image_view, path);
}

int ng_platform_image_view_load_from_data(NGHandle image_view, const unsigned char* data, unsigned int size) {
    return ng_macos_image_view_load_from_data(image_view, data, size);
}

void ng_platform_image_view_set_scaling(NGHandle image_view, int scaling_mode) {
    ng_macos_image_view_set_scaling(image_view, scaling_mode);
}

void ng_platform_image_view_invalidate(NGHandle image_view) {
    ng_macos_image_view_invalidate(image_view);
}

// Slider functions
NGHandle ng_platform_create_slider(double min, double max) {
    return ng_macos_create_slider(min, max);
}

int ng_platform_slider_set_value(NGHandle slider, double value) {
    return ng_macos_slider_set_value(slider, value);
}

double ng_platform_slider_get_value(NGHandle slider) {
    return ng_macos_slider_get_value(slider);
}

int ng_platform_slider_set_enabled(NGHandle slider, int enabled) {
    return ng_macos_slider_set_enabled(slider, enabled);
}

void ng_platform_slider_invalidate(NGHandle slider) {
    ng_macos_slider_invalidate(slider);
}

// Checkbox functions
NGHandle ng_platform_create_checkbox(const char* label) {
    return ng_macos_create_checkbox(label);
}

int ng_platform_checkbox_set_checked(NGHandle checkbox, int checked) {
    return ng_macos_checkbox_set_checked(checkbox, checked);
}

int ng_platform_checkbox_get_checked(NGHandle checkbox) {
    return ng_macos_checkbox_get_checked(checkbox);
}

int ng_platform_checkbox_set_enabled(NGHandle checkbox, int enabled) {
    return ng_macos_checkbox_set_enabled(checkbox, enabled);
}

void ng_platform_checkbox_invalidate(NGHandle checkbox) {
    ng_macos_checkbox_invalidate(checkbox);
}

// ProgressBar functions
NGHandle ng_platform_create_progress_bar(void) {
    return ng_macos_create_progress_bar();
}

int ng_platform_progress_bar_set_value(NGHandle progress_bar, double value) {
    return ng_macos_progress_bar_set_value(progress_bar, value);
}

int ng_platform_progress_bar_set_indeterminate(NGHandle progress_bar, int indeterminate) {
    return ng_macos_progress_bar_set_indeterminate(progress_bar, indeterminate);
}

int ng_platform_progress_bar_set_enabled(NGHandle progress_bar, int enabled) {
    return ng_macos_progress_bar_set_enabled(progress_bar, enabled);
}

void ng_platform_progress_bar_invalidate(NGHandle progress_bar) {
    ng_macos_progress_bar_invalidate(progress_bar);
}

// ComboBox functions
NGHandle ng_platform_create_combo_box(void) {
    return ng_macos_create_combo_box();
}

int ng_platform_combo_box_add_item(NGHandle combo_box, const char* item) {
    return ng_macos_combo_box_add_item(combo_box, item);
}

int ng_platform_combo_box_set_selected(NGHandle combo_box, int index) {
    return ng_macos_combo_box_set_selected(combo_box, index);
}

int ng_platform_combo_box_get_selected(NGHandle combo_box) {
    return ng_macos_combo_box_get_selected(combo_box);
}

int ng_platform_combo_box_clear(NGHandle combo_box) {
    return ng_macos_combo_box_clear(combo_box);
}

int ng_platform_combo_box_set_enabled(NGHandle combo_box, int enabled) {
    return ng_macos_combo_box_set_enabled(combo_box, enabled);
}

void ng_platform_combo_box_invalidate(NGHandle combo_box) {
    ng_macos_combo_box_invalidate(combo_box);
} 