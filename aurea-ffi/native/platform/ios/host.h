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

/* The pen phases PenPhase::from_raw reads. */
#define NG_IOS_PEN_HOVER 0
#define NG_IOS_PEN_DOWN 1
#define NG_IOS_PEN_MOVE 2
#define NG_IOS_PEN_UP 3

/* The app side of the host. Sizes are physical pixels, insets and touches points. `frame` draws
   into a buffer it keeps until the next call, and returns nonzero when it wants another frame. */
typedef struct NGIosHost {
    int (*frame)(unsigned width, unsigned height, float scale, float left, float top, float right,
                 float bottom, const uint32_t** words, unsigned* out_width, unsigned* out_height);
    /* One finger or pencil touching: `id` tells the touches apart while they last, and `pencil`
       is nonzero for an Apple Pencil, whose samples also come through `pen`. */
    void (*touch)(int action, float x, float y, uintptr_t id, int pencil);
    /* An Apple Pencil sample: position in points, pressure 0 to 1, tilt and twist in degrees, age
       in seconds. */
    void (*pen)(int phase, double x, double y, float pressure, float tilt_x, float tilt_y, float twist,
                double age);
    int (*light_theme)(void);
    /* Polled each display refresh while no frame is due; nonzero draws one. Safe to set from any
       thread on the app side. */
    int (*wake)(void);
} NGIosHost;

/* Runs UIKit with the host filling the screen; doesn't return. */
int ng_ios_run_host(NGIosHost host);

#ifdef __cplusplus
}
#endif

#endif
