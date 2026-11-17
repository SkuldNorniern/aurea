#include "touch.h"
#include "common/input.h"

unsigned int ng_linux_touch_bits(GdkInputSource source) {
    return source == GDK_SOURCE_TOUCHSCREEN ? NG_POINTER_TOUCH : 0;
}
