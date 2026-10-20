#ifndef AUREA_COMMON_IMAGE_FIT_H
#define AUREA_COMMON_IMAGE_FIT_H

/* ImageScaling, as numbered on the Rust side. */
#define NG_IMAGE_SCALING_NONE 0
#define NG_IMAGE_SCALING_ASPECT_FIT 1
#define NG_IMAGE_SCALING_ASPECT_FILL 2
#define NG_IMAGE_SCALING_FILL 3

typedef struct {
    double x;
    double y;
    double width;
    double height;
} NGImageRect;

/* Where a `w` x `h` image lands in an `area_w` x `area_h` view, centred. */
static inline NGImageRect ng_image_fit(int mode, double area_w, double area_h, double w, double h) {
    NGImageRect r = {0.0, 0.0, area_w, area_h};
    if (mode == NG_IMAGE_SCALING_FILL || w <= 0.0 || h <= 0.0) return r;
    double sx = area_w / w;
    double sy = area_h / h;
    double scale = 1.0;
    if (mode == NG_IMAGE_SCALING_ASPECT_FIT) scale = sx < sy ? sx : sy;
    else if (mode == NG_IMAGE_SCALING_ASPECT_FILL) scale = sx > sy ? sx : sy;
    r.width = w * scale;
    r.height = h * scale;
    r.x = (area_w - r.width) / 2.0;
    r.y = (area_h - r.height) / 2.0;
    return r;
}

#endif
