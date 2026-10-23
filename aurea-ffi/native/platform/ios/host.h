#ifndef NATIVE_GUI_IOS_HOST_H
#define NATIVE_GUI_IOS_HOST_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NG_IOS_TOUCH_DOWN 0
#define NG_IOS_TOUCH_UP 1
#define NG_IOS_TOUCH_MOVE 2
#define NG_IOS_TOUCH_CANCEL 3

/* The app side of the host. Sizes are physical pixels, insets and touches points. `frame` draws
   into a buffer it keeps until the next call, and returns nonzero when it wants another frame. */
typedef struct NGIosHost {
    int (*frame)(unsigned width, unsigned height, float scale, float left, float top, float right,
                 float bottom, const uint32_t** words, unsigned* out_width, unsigned* out_height);
    void (*touch)(int action, float x, float y);
    int (*light_theme)(void);
} NGIosHost;

/* Runs UIKit with the host filling the screen; doesn't return. */
int ng_ios_run_host(NGIosHost host);

#ifdef __cplusplus
}
#endif

#endif
