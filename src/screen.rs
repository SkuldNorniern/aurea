//! The shape of the screen around the UI: what a touch host knows and a desktop has none of.

use std::sync::Mutex;

use aurea_foundation::lock;

use crate::render::Rect;

const MOST_CUTOUTS: usize = 4;

/// What cuts into the screen, in logical pixels. All zero on the desktop.
///
/// The UI's origin is the safe area's top left, and everything here is measured from the UI, so an
/// app reads it against the size it was given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Screen {
    /// Left, top, right and bottom: the bars and cutouts the UI is already kept inside of.
    pub safe_area: [f32; 4],
    /// Left, top, right and bottom: how far past the safe area the system takes swipes. The UI
    /// still draws there, so a canvas keeps the room and a control stays out. Android only; on iOS
    /// the safe area covers them.
    pub gestures: [f32; 4],
    /// The screen's corner radii: top left, top right, bottom right, bottom left. Android 12 and
    /// later; iOS has no public way to ask, and its safe area keeps clear of most of them.
    pub corners: [f32; 4],
    cutouts: [Rect; MOST_CUTOUTS],
    cutout_count: usize,
}

impl Screen {
    /// A screen with only `safe_area`.
    #[cfg(target_os = "ios")]
    pub(crate) fn safe(safe_area: [f32; 4]) -> Self {
        Self {
            safe_area,
            ..Self::default()
        }
    }

    /// From device pixels measured from the screen's top left, at `scale` pixels per logical one.
    /// `cutouts` are x, y, width and height, one after another.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn from_pixels(
        scale: f32,
        insets: [f32; 4],
        gestures: [f32; 4],
        corners: [f32; 4],
        cutouts: &[f32],
    ) -> Self {
        let scale = scale.max(f32::EPSILON);
        let safe_area = insets.map(|v| v.max(0.0) / scale);
        let mut out = Self {
            safe_area,
            corners: corners.map(|v| v.max(0.0) / scale),
            ..Self::default()
        };
        for (i, past) in out.gestures.iter_mut().enumerate() {
            *past = (gestures[i] - insets[i]).max(0.0) / scale;
        }
        for [x, y, width, height] in cutouts.as_chunks::<4>().0.iter().take(MOST_CUTOUTS) {
            out.cutouts[out.cutout_count] = Rect::new(
                x / scale - safe_area[0],
                y / scale - safe_area[1],
                width / scale,
                height / scale,
            );
            out.cutout_count += 1;
        }
        out
    }

    /// The camera holes and notches, as rects the UI may overlap.
    pub fn cutouts(&self) -> &[Rect] {
        &self.cutouts[..self.cutout_count]
    }

    /// For each of the UI's corners, in [`Self::corners`]' order, how far in along both edges a
    /// control has to start for its corner to be on the screen. 0 where the safe area is enough.
    pub fn corner_clearance(&self) -> [f32; 4] {
        let [left, top, right, bottom] = self.safe_area;
        let sides = [(left, top), (right, top), (right, bottom), (left, bottom)];
        let mut out = [0.0; 4];
        for (i, (x, y)) in sides.into_iter().enumerate() {
            out[i] = clearance(self.corners[i], x, y);
        }
        out
    }
}

/// How far along the diagonal from a point `x` and `y` in from the screen's corner it takes to get
/// inside a corner of radius `radius`.
fn clearance(radius: f32, x: f32, y: f32) -> f32 {
    let (a, b) = (radius - x, radius - y);
    if a <= 0.0 || b <= 0.0 || a * a + b * b <= radius * radius {
        return 0.0;
    }
    let sum = a + b;
    let root = (sum * sum - 2.0 * (a * a + b * b - radius * radius)).max(0.0);
    ((sum - root.sqrt()) / 2.0).max(0.0)
}

static SCREEN: Mutex<Screen> = Mutex::new(Screen {
    safe_area: [0.0; 4],
    gestures: [0.0; 4],
    corners: [0.0; 4],
    cutouts: [Rect::new(0.0, 0.0, 0.0, 0.0); MOST_CUTOUTS],
    cutout_count: 0,
});

/// The screen as the host last reported it.
pub fn screen() -> Screen {
    *lock(&SCREEN)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) fn set(screen: Screen) {
    *lock(&SCREEN) = screen;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_logical_and_measure_from_the_ui() {
        let s = 2.0;
        let screen = Screen::from_pixels(
            s,
            [0.0, 96.0, 0.0, 0.0],
            [32.0, 96.0, 32.0, 84.0],
            [100.0; 4],
            &[500.0, 10.0, 80.0, 80.0],
        );
        assert_eq!(screen.safe_area, [0.0, 48.0, 0.0, 0.0]);
        assert_eq!(
            screen.gestures,
            [16.0, 0.0, 16.0, 42.0],
            "only what is past the safe area"
        );
        assert_eq!(screen.corners, [50.0; 4]);
        assert_eq!(screen.cutouts(), [Rect::new(250.0, -43.0, 40.0, 40.0)]);
    }

    #[test]
    fn extra_cutouts_are_dropped_and_a_bad_count_ignored() {
        let values = [1.0; 26];
        let screen = Screen::from_pixels(1.0, [0.0; 4], [0.0; 4], [0.0; 4], &values);
        assert_eq!(screen.cutouts().len(), MOST_CUTOUTS);
    }

    #[test]
    fn the_desktop_has_nothing_to_keep_clear() {
        let screen = Screen::default();
        assert_eq!(screen.corner_clearance(), [0.0; 4]);
        assert!(screen.cutouts().is_empty());
    }

    #[test]
    fn clearance_lands_on_the_corner_arc() {
        let d = clearance(50.0, 0.0, 0.0);
        let along = 50.0 - d;
        assert!((along * along * 2.0 - 2500.0).abs() < 0.01, "{d}");
        assert!((d - 14.64).abs() < 0.01, "{d}");
    }

    #[test]
    fn the_safe_area_can_clear_a_corner_by_itself() {
        assert_eq!(clearance(50.0, 50.0, 0.0), 0.0, "past it along one edge");
        assert_eq!(clearance(50.0, 15.0, 15.0), 0.0, "already inside the arc");
        assert!(clearance(50.0, 0.0, 20.0) > 0.0);
        assert_eq!(clearance(0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn corners_pair_with_their_own_two_edges() {
        let mut screen = Screen {
            corners: [50.0; 4],
            ..Screen::default()
        };
        screen.safe_area = [0.0, 60.0, 0.0, 0.0];
        let c = screen.corner_clearance();
        assert_eq!(
            (c[0], c[1]),
            (0.0, 0.0),
            "the top bar clears the top corners"
        );
        assert!(c[2] > 14.0 && c[3] > 14.0);
    }
}
