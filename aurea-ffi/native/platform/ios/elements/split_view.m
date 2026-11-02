#import "../elements.h"
#import "../../../common/errors.h"
#import <UIKit/UIKit.h>

/* Two panes with a divider to drag. Neither goes under MIN_PANE while there is room. */

#define MIN_PANE 50.0
#define DIVIDER 1.0
#define GRAB 22.0

@interface NGIOSSplitView : UIView
@property (assign) BOOL stacked;
@property (assign) CGFloat position;
@property (strong) NSMutableArray<UIView*>* panes;
@property (strong) UIView* divider;
@end

@implementation NGIOSSplitView

- (instancetype)initStacked:(BOOL)stacked {
    self = [super initWithFrame:CGRectZero];
    if (self) {
        _stacked = stacked;
        _position = -1;
        _panes = [NSMutableArray array];
        _divider = [[UIView alloc] init];
        _divider.backgroundColor = UIColor.separatorColor;
        [self addSubview:_divider];
        [self addGestureRecognizer:[[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(dragged:)]];
    }
    return self;
}

- (CGFloat)length {
    return _stacked ? self.bounds.size.height : self.bounds.size.width;
}

- (CGFloat)clamped:(CGFloat)position {
    CGFloat most = self.length - DIVIDER - MIN_PANE;
    return most < MIN_PANE ? self.length / 2 : MIN(MAX(position, MIN_PANE), most);
}

- (BOOL)gestureRecognizerShouldBegin:(UIGestureRecognizer*)gesture {
    CGPoint at = [gesture locationInView:self];
    CGFloat along = _stacked ? at.y : at.x;
    return fabs(along - _position) <= GRAB;
}

- (void)dragged:(UIPanGestureRecognizer*)pan {
    CGPoint at = [pan locationInView:self];
    _position = [self clamped:_stacked ? at.y : at.x];
    [self setNeedsLayout];
}

- (void)layoutSubviews {
    [super layoutSubviews];
    if (self.length <= 0) return;
    if (_position < 0) _position = self.length / 2;
    _position = [self clamped:_position];
    CGSize size = self.bounds.size;
    CGFloat second = _position + DIVIDER;
    _divider.hidden = _panes.count < 2;
    if (_stacked) {
        _divider.frame = CGRectMake(0, _position, size.width, DIVIDER);
        if (_panes.count > 0) _panes[0].frame = _panes.count > 1 ? CGRectMake(0, 0, size.width, _position) : self.bounds;
        if (_panes.count > 1) _panes[1].frame = CGRectMake(0, second, size.width, size.height - second);
    } else {
        _divider.frame = CGRectMake(_position, 0, DIVIDER, size.height);
        if (_panes.count > 0) _panes[0].frame = _panes.count > 1 ? CGRectMake(0, 0, _position, size.height) : self.bounds;
        if (_panes.count > 1) _panes[1].frame = CGRectMake(second, 0, size.width - second, size.height);
    }
}

@end

NGHandle ng_platform_create_split_view(int is_vertical) {
    return (__bridge_retained void*)[[NGIOSSplitView alloc] initStacked:is_vertical != 0];
}

int ng_platform_split_view_add(NGHandle split_handle, NGHandle element) {
    if (!split_handle || !element) return NG_ERROR_INVALID_HANDLE;
    NGIOSSplitView* split = (__bridge NGIOSSplitView*)split_handle;
    if (split.panes.count >= 2) return NG_ERROR_INVALID_PARAMETER;
    UIView* view = (__bridge UIView*)element;
    view.translatesAutoresizingMaskIntoConstraints = YES;
    [split.panes addObject:view];
    [split insertSubview:view belowSubview:split.divider];
    [split setNeedsLayout];
    return NG_SUCCESS;
}

int ng_platform_split_view_set_divider_position(NGHandle split_handle, int index, float position) {
    if (!split_handle) return NG_ERROR_INVALID_HANDLE;
    if (index != 0 || !(position >= 0.0f) || !(position < 1e7f)) return NG_ERROR_INVALID_PARAMETER;
    NGIOSSplitView* split = (__bridge NGIOSSplitView*)split_handle;
    split.position = position;
    [split setNeedsLayout];
    return NG_SUCCESS;
}
