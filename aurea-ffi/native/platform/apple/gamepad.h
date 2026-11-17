#ifndef NATIVE_GUI_APPLE_GAMEPAD_H
#define NATIVE_GUI_APPLE_GAMEPAD_H

#include <stdint.h>

/* One pad as GameController sees it now. `buttons` has a bit per button in the order of
   aurea's PadButton places: south east west north, bumpers, stick presses, select start guide,
   dpad up down left right. `axes` are left x y, right x y (-1..1, up positive), then the left
   and right trigger (0..1). */
typedef struct NGPadState {
    uintptr_t key;
    char name[64];
    uint32_t buttons;
    float axes[6];
} NGPadState;

/* Fills up to `max` pads, returns how many. */
int ng_apple_pads_read(NGPadState* out, int max);

#endif
