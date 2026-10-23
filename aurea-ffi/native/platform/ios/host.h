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

/* What the app is going through, for `lifecycle`. */
#define NG_IOS_ACTIVE 0
#define NG_IOS_INACTIVE 1
#define NG_IOS_BACKGROUND 2
#define NG_IOS_FOREGROUND 3
#define NG_IOS_MEMORY_WARNING 4

/* The app side of the host. Sizes are physical pixels, insets and touches points. `frame` is given
   the whole view and its safe area, and draws only the safe area: `words` is that many pixels,
   kept until the next call, or NULL when the picture has not changed. `damage` gets the x, y, width
   and height of the pixels that changed. It returns nonzero when it
   wants another frame. Touches are relative to the view, not the safe area. */
typedef struct NGIosHost {
    int (*frame)(unsigned width, unsigned height, float scale, float left, float top, float right,
                 float bottom, const uint32_t** words, unsigned* out_width, unsigned* out_height,
                 unsigned* damage);
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
    /* The colour around the safe area, 0xRRGGBB. */
    uint32_t (*background)(void);
    /* One of the NG_IOS_ACTIVE ... NG_IOS_MEMORY_WARNING states. */
    void (*lifecycle)(int state);
    /* A hardware keyboard key: an NG_KEY_* code, NG_MOD_* modifiers, and on a press the text it
       types (UTF-8, empty when none). */
    void (*key)(uint32_t code, int pressed, uint32_t modifiers, const char* text);
} NGIosHost;

/* Runs UIKit with the host filling the screen; doesn't return. */
int ng_ios_run_host(NGIosHost host);

/* Restarts the paused display link so `wake` is polled again. Safe from any thread. */
void ng_ios_wake(void);

/* The UIView the GPU draws into, over the safe area, made on first use; touches pass through it.
   Main thread only. */
void* ng_ios_gpu_view(void);

/* Removes the GPU view, when the GPU could not start. Main thread only. */
void ng_ios_drop_gpu_view(void);

#ifdef __cplusplus
}
#endif

#endif
