//! Panning and zooming content inside a rect: a picture in an editor, a map, a node graph, a page.

use std::f32::consts::TAU;

use crate::render::{Point, Rect, Transform};
use crate::ui::widget::scroll::wheel_line_pixels;
use crate::window::{CursorIcon, KeyCode, MouseButton, WindowEvent};

/// Where content sits in a view and how large it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct PanZoom {
    zoom: f32,
    rotation: f32,
    offset: Point,
    view: Rect,
    min_zoom: f32,
    max_zoom: f32,
    /// How much a wheel unit zooms: the zoom is multiplied by `e` to the power of this times the
    /// wheel's travel.
    pub wheel_zoom: f32,
    /// How far a wheel unit scrolls, in logical pixels.
    pub wheel_pan: f32,
    /// Space held turns a left drag into a pan.
    pub space_pans: bool,
    pointer: Option<Point>,
    drag: Option<(Point, MouseButton)>,
    space: bool,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PanZoom {
    /// Content at its own size, its origin at the view's top left, zoom kept between 1/64 and 64.
    pub fn new() -> Self {
        let (wheel_zoom, wheel_pan) = if cfg!(target_os = "macos") {
            (0.01, 1.0)
        } else {
            (0.18, wheel_line_pixels())
        };
        Self {
            zoom: 1.0,
            rotation: 0.0,
            offset: Point::default(),
            view: Rect::default(),
            min_zoom: 1.0 / 64.0,
            max_zoom: 64.0,
            wheel_zoom,
            wheel_pan,
            space_pans: true,
            pointer: None,
            drag: None,
            space: false,
        }
    }

    /// Keeps the zoom between `min` and `max`.
    #[must_use]
    pub fn with_limits(mut self, min: f32, max: f32) -> Self {
        self.min_zoom = min.max(f32::EPSILON);
        self.max_zoom = max.max(self.min_zoom);
        self.zoom = self.clamped(self.zoom);
        self
    }

    /// The screen rect the content shows in.
    pub fn set_view(&mut self, view: Rect) {
        self.view = view;
    }

    /// The view set by `set_view`.
    pub fn view(&self) -> Rect {
        self.view
    }

    /// Screen pixels per content unit.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Where content `(0, 0)` is, from the view's top left.
    pub fn offset(&self) -> Point {
        self.offset
    }

    /// Radians the content is turned, clockwise on the screen.
    pub fn rotation(&self) -> f32 {
        self.rotation
    }

    /// Turns the content to `angle` radians (clockwise), about the middle of the view.
    pub fn set_rotation(&mut self, angle: f32) {
        self.set_rotation_at(self.view.center(), angle);
    }

    /// Turns the content by `angle` radians, keeping the content under the screen point `at` where
    /// it is.
    pub fn rotate_at(&mut self, at: Point, angle: f32) {
        self.set_rotation_at(at, self.rotation + angle);
    }

    /// Turns the content to `angle` radians, keeping the content under the screen point `at` where
    /// it is.
    pub fn set_rotation_at(&mut self, at: Point, angle: f32) {
        if !angle.is_finite() {
            return;
        }
        let held = self.to_content(at);
        self.rotation = angle.rem_euclid(TAU);
        self.hold(held, at);
    }

    /// Whether a drag is moving the content.
    pub fn panning(&self) -> bool {
        self.drag.is_some()
    }

    /// The pointer to show over the view, if panning wants one.
    pub fn cursor(&self) -> Option<CursorIcon> {
        let ready = self.space && self.space_pans && self.pointer_inside();
        (self.panning() || ready).then_some(CursorIcon::Move)
    }

    /// A content point on the screen.
    pub fn to_screen(&self, point: Point) -> Point {
        let turned = self.turn(Point::new(point.x * self.zoom, point.y * self.zoom));
        Point::new(
            self.view.x + self.offset.x + turned.x,
            self.view.y + self.offset.y + turned.y,
        )
    }

    /// The content point under a screen point.
    pub fn to_content(&self, point: Point) -> Point {
        let local = self.unturn(Point::new(
            point.x - self.view.x - self.offset.x,
            point.y - self.view.y - self.offset.y,
        ));
        Point::new(local.x / self.zoom, local.y / self.zoom)
    }

    /// The corners of a content rect on the screen: its top left, top right, bottom right and
    /// bottom left, turned with the view.
    pub fn rect_to_screen_quad(&self, rect: Rect) -> [Point; 4] {
        corners(rect).map(|p| self.to_screen(p))
    }

    /// The corners of a screen rect in content, in the same order as
    /// [`PanZoom::rect_to_screen_quad`].
    pub fn rect_to_content_quad(&self, rect: Rect) -> [Point; 4] {
        corners(rect).map(|p| self.to_content(p))
    }

    /// A content rect on the screen.
    pub fn rect_to_screen(&self, rect: Rect) -> Rect {
        bounds(self.rect_to_screen_quad(rect))
    }

    /// A screen rect in content; on a turned view, the box around its corners.
    pub fn rect_to_content(&self, rect: Rect) -> Rect {
        bounds(self.rect_to_content_quad(rect))
    }

    /// The part of the content the view shows, to draw or load only that.
    pub fn visible(&self) -> Rect {
        self.rect_to_content(self.view)
    }

    /// Draws content coordinates where they belong on the screen; set it on a
    /// [`DrawingContext`](crate::render::DrawingContext) between `save` and `restore`, with a clip
    /// to [`PanZoom::view`].
    pub fn transform(&self) -> Transform {
        let origin = self.to_screen(Point::default());
        Transform::scale(self.zoom, self.zoom)
            .multiply(Transform::rotate(self.rotation))
            .multiply(Transform::translate(origin.x, origin.y))
    }

    /// Zooms by `factor`, keeping the content under the screen point `at` where it is.
    pub fn zoom_at(&mut self, at: Point, factor: f32) {
        self.set_zoom_at(at, self.zoom * factor);
    }

    /// Sets the zoom, keeping the content under the screen point `at` where it is.
    pub fn set_zoom_at(&mut self, at: Point, zoom: f32) {
        let held = self.to_content(at);
        self.zoom = self.clamped(zoom);
        self.hold(held, at);
    }

    /// Screen pixels are logical, so on a display at `scale` a zoom of 1 draws each content unit
    /// over `scale` device pixels and an image never shows sharp.
    pub fn device_zoom(&self, scale: f32) -> f32 {
        self.zoom * scale.max(f32::EPSILON)
    }

    /// Sets the zoom in device pixels per content unit (1 for actual pixels), keeping the content
    /// under `at` where it is, then [`snaps`](Self::snap_to_device_pixels) it to the device grid.
    pub fn set_device_zoom_at(&mut self, at: Point, device_zoom: f32, scale: f32) {
        let scale = scale.max(f32::EPSILON);
        self.set_zoom_at(at, device_zoom / scale);
        self.snap_to_device_pixels(scale);
    }

    /// Moves the content by under a device pixel so its origin lands on a whole device pixel.
    pub fn snap_to_device_pixels(&mut self, scale: f32) {
        if self.rotation != 0.0 || scale <= 0.0 {
            return;
        }
        let snap = |view: f32, offset: f32| ((view + offset) * scale).round() / scale - view;
        self.offset = Point::new(
            snap(self.view.x, self.offset.x),
            snap(self.view.y, self.offset.y),
        );
    }

    fn hold(&mut self, held: Point, at: Point) {
        let turned = self.turn(Point::new(held.x * self.zoom, held.y * self.zoom));
        self.offset = Point::new(at.x - self.view.x - turned.x, at.y - self.view.y - turned.y);
    }

    /// Moves the content by screen pixels.
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.offset = Point::new(self.offset.x + dx, self.offset.y + dy);
    }

    /// Puts a content point in the middle of the view.
    pub fn center_on(&mut self, point: Point) {
        self.hold(point, self.view.center());
    }

    /// Zooms so content `width` by `height`, turned as the view is, fits the view with `margin`
    /// pixels around it, and centres it.
    pub fn fit(&mut self, width: f32, height: f32, margin: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let (sin, cos) = self.rotation.sin_cos();
        let (sin, cos) = (sin.abs(), cos.abs());
        let (turned_w, turned_h) = (width * cos + height * sin, width * sin + height * cos);
        let room_w = (self.view.width - 2.0 * margin).max(1.0);
        let room_h = (self.view.height - 2.0 * margin).max(1.0);
        self.zoom = self.clamped((room_w / turned_w).min(room_h / turned_h));
        self.center_on(Point::new(width / 2.0, height / 2.0));
    }

    /// Takes one window event.
    pub fn handle(&mut self, event: &WindowEvent) -> bool {
        match *event {
            WindowEvent::MouseMove { x, y, .. } => self.moved(point(x, y)),
            WindowEvent::MouseExited => {
                self.pointer = None;
                false
            }
            WindowEvent::MouseButton {
                button,
                pressed,
                x,
                y,
                ..
            } => self.button(button, pressed, point(x, y)),
            WindowEvent::MouseWheel {
                delta_x,
                delta_y,
                modifiers,
            } => self.wheel(
                to_f32(delta_x),
                to_f32(delta_y),
                modifiers.command(),
                modifiers.shift,
            ),
            WindowEvent::KeyInput {
                key: KeyCode::Space,
                pressed,
                ..
            } => {
                let was = self.cursor();
                self.space = pressed;
                self.cursor() != was
            }
            WindowEvent::Unfocused => {
                self.space = false;
                self.drag.take().is_some()
            }
            _ => false,
        }
    }

    fn moved(&mut self, at: Point) -> bool {
        self.pointer = Some(at);
        let Some((last, held)) = self.drag else {
            return false;
        };
        self.pan_by(at.x - last.x, at.y - last.y);
        self.drag = Some((at, held));
        true
    }

    fn button(&mut self, button: MouseButton, pressed: bool, at: Point) -> bool {
        self.pointer = Some(at);
        let pans = match button {
            MouseButton::Middle => true,
            MouseButton::Left => self.space && self.space_pans,
            _ => false,
        };
        if !pressed {
            let ends = self.drag.is_some_and(|(_, held)| held == button);
            if ends {
                self.drag = None;
            }
            return ends;
        }
        if pans && self.drag.is_none() && self.view.contains(at) {
            self.drag = Some((at, button));
            return true;
        }
        false
    }

    fn wheel(&mut self, dx: f32, dy: f32, zoom: bool, sideways: bool) -> bool {
        let Some(at) = self.pointer.filter(|at| self.view.contains(*at)) else {
            return false;
        };
        if zoom {
            self.zoom_at(at, (dy * self.wheel_zoom).exp());
            return true;
        }
        let (dx, dy) = if sideways && dx == 0.0 {
            (-dy, 0.0)
        } else {
            (dx, dy)
        };
        self.pan_by(-dx * self.wheel_pan, dy * self.wheel_pan);
        true
    }

    fn turn(&self, v: Point) -> Point {
        let (sin, cos) = self.rotation.sin_cos();
        Point::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    }

    fn unturn(&self, v: Point) -> Point {
        let (sin, cos) = self.rotation.sin_cos();
        Point::new(v.x * cos + v.y * sin, -v.x * sin + v.y * cos)
    }

    fn pointer_inside(&self) -> bool {
        self.pointer.is_some_and(|at| self.view.contains(at))
    }

    fn clamped(&self, zoom: f32) -> f32 {
        if zoom.is_finite() {
            zoom.clamp(self.min_zoom, self.max_zoom)
        } else {
            self.zoom
        }
    }
}

fn corners(rect: Rect) -> [Point; 4] {
    let (r, b) = (rect.x + rect.width, rect.y + rect.height);
    [
        Point::new(rect.x, rect.y),
        Point::new(r, rect.y),
        Point::new(r, b),
        Point::new(rect.x, b),
    ]
}

fn bounds(points: [Point; 4]) -> Rect {
    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points {
        (x0, y0) = (x0.min(p.x), y0.min(p.y));
        (x1, y1) = (x1.max(p.x), y1.max(p.y));
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn point(x: f64, y: f64) -> Point {
    Point::new(to_f32(x), to_f32(y))
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(v: f64) -> f32 {
    v as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{Modifiers, MouseButtons};
    use std::f32::consts::FRAC_PI_2;

    const VIEW: Rect = Rect::new(100.0, 50.0, 400.0, 300.0);

    fn view() -> PanZoom {
        let mut pz = PanZoom::new();
        pz.set_view(VIEW);
        pz
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    fn mv(x: f64, y: f64) -> WindowEvent {
        WindowEvent::MouseMove {
            x,
            y,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
            time: Default::default(),
        }
    }

    fn button(button: MouseButton, pressed: bool, x: f64, y: f64) -> WindowEvent {
        WindowEvent::MouseButton {
            button,
            pressed,
            modifiers: Modifiers::default(),
            x,
            y,
            click_count: 1,
            time: Default::default(),
        }
    }

    fn wheel(delta_y: f64, modifiers: Modifiers) -> WindowEvent {
        WindowEvent::MouseWheel {
            delta_x: 0.0,
            delta_y,
            modifiers,
        }
    }

    #[test]
    fn screen_and_content_points_round_trip() {
        let mut pz = view();
        pz.zoom_at(Point::new(130.0, 70.0), 3.0);
        pz.pan_by(12.0, -7.0);
        let p = Point::new(42.5, -3.25);
        assert!(close(pz.to_content(pz.to_screen(p)), p));
        assert!(close(pz.transform().map_point(p), pz.to_screen(p)));
        let r = pz.rect_to_content(pz.rect_to_screen(Rect::new(1.0, 2.0, 3.0, 4.0)));
        assert!((r.width - 3.0).abs() < 1e-4 && (r.height - 4.0).abs() < 1e-4);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut pz = view();
        let at = Point::new(260.0, 180.0);
        let under = pz.to_content(at);
        pz.zoom_at(at, 2.5);
        assert!(close(pz.to_content(at), under));
        pz.zoom_at(at, 1e9);
        assert_eq!(pz.zoom(), 64.0, "held to the limit");
        assert!(close(pz.to_content(at), under));
    }

    #[test]
    fn fitting_centres_the_content() {
        let mut pz = view();
        pz.fit(800.0, 200.0, 0.0);
        assert_eq!(pz.zoom(), 0.5);
        assert!(close(pz.to_screen(Point::new(400.0, 100.0)), VIEW.center()));
        assert!(close(pz.visible().center(), Point::new(400.0, 100.0)));
    }

    #[test]
    fn the_wheel_scrolls_and_zooms_over_the_view_only() {
        let mut pz = view();
        assert!(
            !pz.handle(&wheel(1.0, Modifiers::default())),
            "no pointer yet"
        );
        pz.handle(&mv(200.0, 100.0));
        assert!(pz.handle(&wheel(1.0, Modifiers::default())));
        assert!(pz.offset().y > 0.0, "up shows what is above");
        let sideways = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        pz.handle(&wheel(-1.0, sideways));
        assert!(pz.offset().x < 0.0, "Shift and down goes right");
        let command = Modifiers {
            ctrl: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Modifiers::default()
        };
        pz.handle(&wheel(1.0, command));
        assert!(pz.zoom() > 1.0);
        pz.handle(&mv(10.0, 10.0));
        assert!(!pz.handle(&wheel(1.0, command)), "outside the view");
    }

    #[test]
    fn a_middle_drag_pans() {
        let mut pz = view();
        assert!(pz.handle(&button(MouseButton::Middle, true, 200.0, 100.0)));
        assert!(pz.handle(&mv(230.0, 90.0)));
        assert!(
            !pz.handle(&button(MouseButton::Left, false, 230.0, 90.0)),
            "another button's release"
        );
        assert!(pz.panning());
        assert!(pz.handle(&button(MouseButton::Middle, false, 230.0, 90.0)));
        assert_eq!(pz.offset(), Point::new(30.0, -10.0));
    }

    #[test]
    fn space_turns_a_left_drag_into_a_pan() {
        let mut pz = view();
        pz.handle(&mv(200.0, 100.0));
        assert!(
            !pz.handle(&button(MouseButton::Left, true, 200.0, 100.0)),
            "a tool's press"
        );
        let space = |pressed| WindowEvent::KeyInput {
            key: KeyCode::Space,
            pressed,
            modifiers: Modifiers::default(),
        };
        assert!(
            pz.handle(&space(true)),
            "the pointer shows panning is ready"
        );
        assert_eq!(pz.cursor(), Some(CursorIcon::Move));
        assert!(pz.handle(&button(MouseButton::Left, true, 200.0, 100.0)));
        pz.handle(&space(false));
        pz.handle(&mv(205.0, 100.0));
        assert!(pz.handle(&button(MouseButton::Left, false, 205.0, 100.0)));
        assert_eq!(pz.offset(), Point::new(5.0, 0.0));
        assert_eq!(pz.cursor(), None);
    }

    #[test]
    fn a_turned_view_maps_points_rects_and_the_transform_alike() {
        let mut pz = view();
        pz.fit(200.0, 100.0, 10.0);
        let middle = pz.to_content(VIEW.center());
        pz.set_rotation(FRAC_PI_2);
        assert!(
            close(pz.to_content(VIEW.center()), middle),
            "turns about the middle"
        );
        let p = Point::new(30.0, -12.5);
        assert!(close(pz.to_content(pz.to_screen(p)), p));
        assert!(close(pz.transform().map_point(p), pz.to_screen(p)));
        let a = pz.to_screen(Point::new(0.0, 0.0));
        let b = pz.to_screen(Point::new(10.0, 0.0));
        assert!(close(
            Point::new(b.x - a.x, b.y - a.y),
            Point::new(0.0, 10.0 * pz.zoom())
        ));
        let r = Rect::new(5.0, 6.0, 40.0, 20.0);
        let quad = pz.rect_to_screen_quad(r);
        assert!(close(quad[2], pz.to_screen(Point::new(45.0, 26.0))));
        let back = pz.rect_to_content(pz.rect_to_screen(r));
        assert!(close(Point::new(back.x, back.y), Point::new(5.0, 6.0)));
        assert!((back.width - 40.0).abs() < 1e-3 && (back.height - 20.0).abs() < 1e-3);
    }

    #[test]
    fn turning_and_zooming_hold_the_point_under_the_pointer() {
        let mut pz = view();
        let at = Point::new(170.0, 230.0);
        let under = pz.to_content(at);
        pz.rotate_at(at, 0.7);
        assert!(close(pz.to_content(at), under));
        pz.zoom_at(at, 2.0);
        assert!(close(pz.to_content(at), under));
        pz.rotate_at(at, TAU);
        assert!((pz.rotation() - 0.7).abs() < 1e-4, "wraps a full turn");
        assert!(close(pz.to_content(at), under));
    }

    #[test]
    fn fitting_a_turned_view_fits_its_turned_size() {
        let mut pz = view();
        pz.set_rotation(FRAC_PI_2);
        pz.fit(300.0, 800.0, 0.0);
        assert!((pz.zoom() - 0.5).abs() < 1e-4, "{}", pz.zoom());
        assert!(close(pz.to_screen(Point::new(150.0, 400.0)), VIEW.center()));
    }

    #[test]
    fn actual_pixels_put_one_content_unit_on_one_device_pixel() {
        let mut pz = view();
        pz.pan_by(0.37, 0.81);
        pz.set_device_zoom_at(Point::new(250.0, 200.0), 1.0, 1.5);
        assert!((pz.zoom() - 1.0 / 1.5).abs() < 1e-6);
        assert!((pz.device_zoom(1.5) - 1.0).abs() < 1e-6);
        for x in [0.0, 1.0, 17.0, 333.0] {
            let device = pz.to_screen(Point::new(x, x)).x * 1.5;
            assert!(
                (device - device.round()).abs() < 1e-3,
                "unit {x} at {device}"
            );
        }
        pz.set_device_zoom_at(Point::new(250.0, 200.0), 2.0, 1.25);
        let device = pz.to_screen(Point::new(5.0, 0.0)).x * 1.25;
        assert!((device - device.round()).abs() < 1e-3, "{device}");
    }
}
