#ifndef NATIVE_GUI_IOS_ACCESSIBILITY_H
#define NATIVE_GUI_IOS_ACCESSIBILITY_H

#include <stddef.h>
#include <stdint.h>

/* Traits of a drawn element, matched to UIKit's in accessibility.m. */
#define NG_AX_BUTTON (1u << 0)
#define NG_AX_LINK (1u << 1)
#define NG_AX_HEADER (1u << 2)
#define NG_AX_STATIC_TEXT (1u << 3)
#define NG_AX_ADJUSTABLE (1u << 4)
#define NG_AX_SELECTED (1u << 5)
#define NG_AX_NOT_ENABLED (1u << 6)
#define NG_AX_TEXT_FIELD (1u << 7)
#define NG_AX_IMAGE (1u << 8)
#define NG_AX_TOGGLE (1u << 9)

/* What VoiceOver asked of an element. */
#define NG_AX_ACTIVATE 0
#define NG_AX_INCREMENT 1
#define NG_AX_DECREMENT 2

/* One element the UI drew, in the UI's points from the safe area's corner. Strings are UTF-8,
   copied before ng_ios_ax_update returns. */
typedef struct {
    uint64_t id;
    float x, y, width, height;
    uint32_t traits;
    const char* label;
    const char* value;
    const char* hint;
} NGAxElement;

typedef void (*NGAxAction)(uint64_t id, int kind);
/* A screen reader wants the elements: the UI starts describing itself. */
typedef void (*NGAxActivate)(void);

void ng_ios_ax_set_handler(NGAxAction action, NGAxActivate activate);
/* The elements in reading order, and the one with the app's focus (0 for none). */
void ng_ios_ax_update(const NGAxElement* elements, size_t count, uint64_t focus);

#endif
