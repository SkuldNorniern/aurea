//! Panning and zooming content inside a rect: a picture in an editor, a
//! map, a node graph, a page.
//!
//! [`PanZoom`] maps between the screen and the content's own coordinates
//! and follows the usual editor input: the wheel scrolls, Shift turns it
//! sideways, the platform's shortcut key with the wheel zooms where the
//! pointer is, and the middle button or Space with the left button drags.
//! It reads window events itself, so it works with or without a [`Ui`].
//!
//! [`Ui`]: crate::ui::Ui

use crate::render::{Point, Rect, Transform};
use crate::window::{CursorIcon, KeyCode, MouseButton, WindowEvent};

/// Where content sits in a view and how large it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct PanZoom {
    zoom: f32,
    /// Where content `(0, 0)` lands, from the view's top left.
    offset: Point,
    view: Rect,
    min_zoom: f32,
    max_zoom: f32,
    /// How much a wheel unit zooms: the zoom is multiplied by `e` to the
    /// power of this times the wheel's travel.
    pub wheel_zoom: f32,
    /// How far a wheel unit scrolls, in logical pixels.
    pub wheel_pan: f32,
    /// Space held turns a left drag into a pan.
    pub space_pans: bool,
    pointer: Option<Point>,
    /// The last point of a pan and the button that holds it.
    drag: Option<(Point, MouseButton)>,
    space: bool,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PanZoom {
    /// Content at its own size, its origin at the view's top left, zoom
    /// kept between 1/64 and 64.
    pub fn new() -> Self {
        // macOS hands over a trackpad's travel in points, many per gesture;
        // elsewhere a wheel notch is 1.0.
        let (wheel_zoom, wheel_pan) = if cfg!(target_os = "macos") {
            (0.01, 1.0)
        } else {
            (0.18, 48.0)
        };
        Self {
            zoom: 1.0,
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

    /// The screen rect the content shows in. Call it each frame, before
    /// reading positions; resizing it keeps the content's top left in place.
    pub fn set_view(&mut self, view: Rect) {
        self.view = view;
    }

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
        Point::new(
            self.view.x + self.offset.x + point.x * self.zoom,
            self.view.y + self.offset.y + point.y * self.zoom,
        )
    }

    /// The content point under a screen point.
    pub fn to_content(&self, point: Point) -> Point {
        Point::new(
            (point.x - self.view.x - self.offset.x) / self.zoom,
            (point.y - self.view.y - self.offset.y) / self.zoom,
        )
    }

    pub fn rect_to_screen(&self, rect: Rect) -> Rect {
        let at = self.to_screen(Point::new(rect.x, rect.y));
        Rect::new(at.x, at.y, rect.width * self.zoom, rect.height * self.zoom)
    }

    pub fn rect_to_content(&self, rect: Rect) -> Rect {
        let at = self.to_content(Point::new(rect.x, rect.y));
        Rect::new(at.x, at.y, rect.width / self.zoom, rect.height / self.zoom)
    }

    /// The part of the content the view shows, to draw or load only that.
    pub fn visible(&self) -> Rect {
        self.rect_to_content(self.view)
    }

    /// Draws content coordinates where they belong on the screen; set it on
    /// a [`DrawingContext`](crate::render::DrawingContext) between `save`
    /// and `restore`, with a clip to [`PanZoom::view`].
    pub fn transform(&self) -> Transform {
        let origin = self.to_screen(Point::default());
        Transform::scale(self.zoom, self.zoom).multiply(Transform::translate(origin.x, origin.y))
    }

    /// Zooms by `factor`, keeping the content under the screen point `at`
    /// where it is.
    pub fn zoom_at(&mut self, at: Point, factor: f32) {
        self.set_zoom_at(at, self.zoom * factor);
    }

    /// Sets the zoom, keeping the content under the screen point `at` where
    /// it is.
    pub fn set_zoom_at(&mut self, at: Point, zoom: f32) {
        let held = self.to_content(at);
        self.zoom = self.clamped(zoom);
        self.offset = Point::new(
            at.x - self.view.x - held.x * self.zoom,
            at.y - self.view.y - held.y * self.zoom,
        );
    }

    /// Moves the content by screen pixels.
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.offset = Point::new(self.offset.x + dx, self.offset.y + dy);
    }

    /// Puts a content point in the middle of the view.
    pub fn center_on(&mut self, point: Point) {
        self.offset = Point::new(
            self.view.width / 2.0 - point.x * self.zoom,
            self.view.height / 2.0 - point.y * self.zoom,
        );
    }

    /// Zooms so content `width` by `height` fits the view with `margin`
    /// pixels around it, and centres it.
    pub fn fit(&mut self, width: f32, height: f32, margin: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let room_w = (self.view.width - 2.0 * margin).max(1.0);
        let room_h = (self.view.height - 2.0 * margin).max(1.0);
        self.zoom = self.clamped((room_w / width).min(room_h / height));
        self.center_on(Point::new(width / 2.0, height / 2.0));
    }

    /// Takes one window event. Returns whether the view moved or its
    /// pointer changed; a press it returns `true` for starts a pan and is
    /// not for the app's own tools.
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
        // The wheel turned up shows what is above; Shift gives a plain wheel
        // the sideways travel a touchpad already has.
        let (dx, dy) = if sideways && dx == 0.0 {
            (-dy, 0.0)
        } else {
            (dx, dy)
        };
        self.pan_by(-dx * self.wheel_pan, dy * self.wheel_pan);
        true
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

fn point(x: f64, y: f64) -> Point {
    Point::new(to_f32(x), to_f32(y))
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(v: f64) -> f32 {
    // Pointer positions and wheel steps are well inside f32's range.
    v as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{Modifiers, MouseButtons};

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
}
