#import "gamepad.h"
#import <GameController/GameController.h>
#import <objc/runtime.h>
#include <string.h>

/* A number per controller for as long as it lives, never given again: the pointer could be. */
static char kPadKey;

static uintptr_t pad_key(GCController* pad) {
    static uintptr_t next = 1;
    NSNumber* key = objc_getAssociatedObject(pad, &kPadKey);
    if (!key) {
        key = @(next++);
        objc_setAssociatedObject(pad, &kPadKey, key, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
    return (uintptr_t)key.unsignedLongValue;
}

static void press(uint32_t* bits, int place, GCControllerButtonInput* button) {
    if (button.isPressed) *bits |= 1u << place;
}

int ng_apple_pads_read(NGPadState* out, int max) {
    /* No background monitoring: input/gamepad reports pads only while a window is focused. */
    int count = 0;
    for (GCController* pad in GCController.controllers) {
        if (count >= max) break;
        GCExtendedGamepad* g = pad.extendedGamepad;
        if (!g) continue;
        NGPadState* s = &out[count++];
        memset(s, 0, sizeof *s);
        s->key = pad_key(pad);
        const char* name = pad.vendorName.UTF8String ?: "Gamepad";
        strncpy(s->name, name, sizeof s->name - 1);
        uint32_t bits = 0;
        press(&bits, 0, g.buttonA);
        press(&bits, 1, g.buttonB);
        press(&bits, 2, g.buttonX);
        press(&bits, 3, g.buttonY);
        press(&bits, 4, g.leftShoulder);
        press(&bits, 5, g.rightShoulder);
        press(&bits, 6, g.leftThumbstickButton);
        press(&bits, 7, g.rightThumbstickButton);
        press(&bits, 8, g.buttonOptions);
        press(&bits, 9, g.buttonMenu);
        if (@available(macOS 11.0, iOS 14.0, *)) press(&bits, 10, g.buttonHome);
        press(&bits, 11, g.dpad.up);
        press(&bits, 12, g.dpad.down);
        press(&bits, 13, g.dpad.left);
        press(&bits, 14, g.dpad.right);
        s->buttons = bits;
        s->axes[0] = g.leftThumbstick.xAxis.value;
        s->axes[1] = g.leftThumbstick.yAxis.value;
        s->axes[2] = g.rightThumbstick.xAxis.value;
        s->axes[3] = g.rightThumbstick.yAxis.value;
        s->axes[4] = g.leftTrigger.value;
        s->axes[5] = g.rightTrigger.value;
    }
    return count;
}
