#import "ime.h"
#import "host_view.h"

/* The composition as the app sees it, the cursor in UTF-8 bytes. */
void ng_ios_ime_preedit(NSString* marked, NSInteger caret) {
    if (!g_ios_host.preedit) return;
    caret = MIN(MAX(caret, 0), (NSInteger)marked.length);
    NSString* head = [marked substringToIndex:(NSUInteger)caret];
    int cursor = marked.length ? (int)[head lengthOfBytesUsingEncoding:NSUTF8StringEncoding] : -1;
    g_ios_host.preedit(marked.UTF8String ?: "", cursor);
}
