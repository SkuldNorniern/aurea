#import "../../common/errors.h"
#import "../../common/platform_api.h"

/* iOS has no notification area. */
NGHandle ng_platform_create_tray(const char* tooltip, unsigned int click_id) {
    (void)tooltip; (void)click_id;
    return NULL;
}

void ng_platform_destroy_tray(NGHandle tray) {
    (void)tray;
}

int ng_platform_tray_set_icon_rgba(NGHandle tray, const unsigned char* rgba, unsigned int width, unsigned int height) {
    (void)tray; (void)rgba; (void)width; (void)height;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_set_tooltip(NGHandle tray, const char* tooltip) {
    (void)tray; (void)tooltip;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_add_item(NGHandle tray, const char* title, unsigned int id) {
    (void)tray; (void)title; (void)id;
    return NG_ERROR_UNSUPPORTED;
}

int ng_platform_tray_notify(NGHandle tray, const char* title, const char* body) {
    (void)tray; (void)title; (void)body;
    return NG_ERROR_UNSUPPORTED;
}
