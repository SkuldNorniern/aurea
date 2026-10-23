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
    /* A key: an NG_KEY_* code, NG_MOD_* modifiers, and on a press the text it types (UTF-8, empty
       when none). Text from the on-screen keyboard comes with NG_KEY_UNKNOWN: text alone. */
    void (*key)(uint32_t code, int pressed, uint32_t modifiers, const char* text);
    /* Text an input method is composing (UTF-8, empty when it stops) and the cursor in it in
       bytes, -1 for none. */
    void (*preedit)(const char* text, int cursor);
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

/* Shows the on-screen keyboard, or hides it. Main thread only. */
void ng_ios_show_keyboard(int show);

/* Where text is being typed, in points from the safe area's top left, for input methods to show
   their candidates beside. Main thread only. */
void ng_ios_set_text_area(float x, float y, float width, float height);

/* How many points of the view the on-screen keyboard covers from the bottom, 0 when it is down. */
float ng_ios_keyboard_height(void);

/* Hears the files a document picker chose: NUL-terminated paths ended by an empty one, none when
   the user cancelled. On the main thread, once. */
typedef void (*NGIosPicked)(void* ctx, const char* paths);

/* Lets the user choose files to open, of the NUL-separated `extensions` (any, when empty); they
   are copied into the app's sandbox. Returns 0 when it cannot show, and `done` is not called. */
int ng_ios_pick_files(const char* extensions, int multiple, NGIosPicked done, void* ctx);

/* Hears the answer to an alert or sheet. On the main thread, once. */
typedef void (*NGIosAnswered)(void* ctx, int answer);

/* An alert with `buttons` as MessageButtons codes; `done` hears the MessageChoice code. Returns 0
   when it cannot show, and `done` is not called. */
int ng_ios_alert(const char* title, const char* message, int buttons, NGIosAnswered done, void* ctx);

/* The share sheet for the NUL-separated `paths`, pointing at the given rectangle in points from the
   safe area's top left on an iPad (centred when it is empty); `done` hears 1 when they went
   somewhere. Returns 0 when it cannot show, and `done` is not called. */
int ng_ios_share_files(const char* paths, float x, float y, float width, float height, NGIosAnswered done,
                       void* ctx);

/* Lets the user save copies of the NUL-separated `paths` elsewhere, such as iCloud Drive; `done`
   hears where they went. Returns 0 when it cannot show, and `done` is not called. */
int ng_ios_export_files(const char* paths, NGIosPicked done, void* ctx);

#ifdef __cplusplus
}
#endif

#endif
