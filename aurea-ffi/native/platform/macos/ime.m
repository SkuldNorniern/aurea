#import "ime.h"
#import "content_view.h"
#import "window.h"
#import "common/errors.h"
#import "common/rust_callbacks.h"

void ng_macos_ime_preedit(AureaContentView* view, NSString* text, NSRange selected) {
    if (!view.windowHandle) return;
    if (!text.length) {
        ng_invoke_ime_preedit(view.windowHandle, "", -1);
        return;
    }
    int cursor = -1;
    if (selected.location != NSNotFound && selected.location <= text.length) {
        NSString* head = [text substringToIndex:selected.location];
        cursor = (int)[head lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
    }
    const char* utf8 = [text UTF8String];
    ng_invoke_ime_preedit(view.windowHandle, utf8 ? utf8 : "", cursor);
}

int ng_macos_window_set_ime_cursor_area(NGHandle window, float x, float y, float width, float height) {
    if (!window) return NG_ERROR_INVALID_HANDLE;
    NSView* view = [(__bridge NSWindow*)window contentView];
    if (![view isKindOfClass:[AureaContentView class]]) return NG_ERROR_PLATFORM_SPECIFIC;
    ((AureaContentView*)view).imeArea = NSMakeRect(x, y, width, height);
    [[view inputContext] invalidateCharacterCoordinates];
    return NG_SUCCESS;
}
