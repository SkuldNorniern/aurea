//! An axis: what range it shows and where its ticks go.

use crate::render::Color;

use super::scale::{Range, Scale};
use super::ticks::{Tick, TickPlan};

/// How an axis decides what range to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bounds {
    /// A range the caller sets and the plot never changes.
    Fixed(Range),
    /// Grows to hold the data, with `padding` as a fraction of the span.
    Auto {
        /// Margin as a fraction of the span.
        padding: f64,
    },
    /// Follows the newest data, keeping a window this wide.
    Window {
        /// Width of the window in data units.
        span: f64,
    },
}

impl Bounds {
    pub(super) fn needs_data(self) -> bool {
        !matches!(self, Self::Fixed(_))
    }
}

impl Default for Bounds {
    fn default() -> Self {
        Self::Auto { padding: 0.05 }
    }
}

/// One axis of a plot.
#[derive(Debug, Clone)]
pub struct Axis {
    /// How the range is chosen.
    pub bounds: Bounds,
    /// Linear or log scale.
    pub scale: Scale,
    /// Tick spacing and count.
    pub ticks: TickPlan,
    /// Written alongside the axis.
    pub label: String,
    resolved: Range,
    resolved_ticks: Vec<Tick>,
    ticked: Option<(Range, Scale, TickPlan)>,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            scale: Scale::Linear,
            ticks: TickPlan::default(),
            label: String::new(),
            resolved: Range::new(0.0, 1.0),
            resolved_ticks: Vec::new(),
            ticked: None,
        }
    }
}

impl Axis {
    /// An axis pinned to a range.
    pub fn fixed(min: f64, max: f64) -> Self {
        Self {
            bounds: Bounds::Fixed(Range::new(min, max)),
            resolved: Range::new(min, max),
            ..Self::default()
        }
    }

    /// An axis that follows the data.
    pub fn auto() -> Self {
        Self::default()
    }

    /// An axis that keeps a window of the newest data.
    pub fn window(span: f64) -> Self {
        Self {
            bounds: Bounds::Window { span },
            ..Self::default()
        }
    }

    /// Sets the scale.
    pub fn with_scale(mut self, scale: Scale) -> Self {
        self.scale = scale;
        self
    }

    /// Sets the tick plan.
    pub fn with_ticks(mut self, ticks: TickPlan) -> Self {
        self.ticks = ticks;
        self
    }

    /// Sets the label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// The range currently drawn.
    pub fn range(&self) -> Range {
        self.resolved
    }

    pub(super) fn resolve(&mut self, data: Option<Range>, view: Option<Range>) {
        self.resolved = match (view, self.bounds) {
            (Some(view), _) => view,
            (None, Bounds::Fixed(range)) => range,
            (None, Bounds::Auto { padding }) => match data {
                Some(range) => range.padded(padding).or_widened(1.0),
                None => self.resolved,
            },
            (None, Bounds::Window { span }) => match data {
                Some(range) => Range::new(range.max - span.abs(), range.max),
                None => self.resolved,
            },
        };
        if self.scale == Scale::Log10 && self.resolved.min <= 0.0 {
            let max = if self.resolved.max > 0.0 {
                self.resolved.max
            } else {
                1.0
            };
            self.resolved = Range::new(max / 1000.0, max);
        }
        let stale = self.ticked.as_ref().is_none_or(|(range, scale, plan)| {
            *range != self.resolved || *scale != self.scale || *plan != self.ticks
        });
        if stale {
            self.resolved_ticks = self.ticks.ticks(self.resolved, self.scale);
            self.ticked = Some((self.resolved, self.scale, self.ticks.clone()));
        }
    }

    /// The ticks for the range in use.
    pub fn resolved_ticks(&self) -> &[Tick] {
        &self.resolved_ticks
    }
}

/// A vertical or horizontal line at a value, with a readout.
#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    /// Where it sits, in data values on its own axis.
    pub value: f64,
    /// Vertical cursors sit at an x value, horizontal ones at a y value.
    pub vertical: bool,
    /// `None` takes the style's marker colour.
    pub color: Option<Color>,
    /// Written next to the cursor.
    pub label: String,
}

impl Cursor {
    /// A vertical cursor at `value`.
    pub fn vertical(value: f64) -> Self {
        Self {
            value,
            vertical: true,
            color: None,
            label: String::new(),
        }
    }

    /// A horizontal cursor at `value`.
    pub fn horizontal(value: f64) -> Self {
        Self {
            vertical: false,
            ..Self::vertical(value)
        }
    }

    /// Sets the colour.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(mut axis: Axis, data: Option<Range>) -> Axis {
        axis.resolve(data, None);
        axis
    }

    #[test]
    fn an_auto_axis_grows_to_the_data() {
        let axis = resolved(Axis::auto(), Some(Range::new(0.0, 10.0)));
        assert!(axis.range().min < 0.0, "padded below");
        assert!(axis.range().max > 10.0, "padded above");
    }

    #[test]
    fn an_auto_axis_with_no_data_keeps_what_it_had() {
        let mut axis = Axis::fixed(2.0, 8.0);
        axis.bounds = Bounds::Auto { padding: 0.0 };
        assert_eq!(resolved(axis, None).range(), Range::new(2.0, 8.0));
    }

    #[test]
    fn a_fixed_axis_ignores_the_data() {
        let axis = resolved(Axis::fixed(0.0, 1.0), Some(Range::new(-100.0, 100.0)));
        assert_eq!(axis.range(), Range::new(0.0, 1.0));
    }

    #[test]
    fn a_window_axis_follows_the_newest_value() {
        let axis = resolved(Axis::window(10.0), Some(Range::new(0.0, 55.0)));
        assert_eq!(axis.range(), Range::new(45.0, 55.0));
    }

    #[test]
    fn a_flat_auto_axis_is_widened_so_a_constant_signal_shows() {
        let axis = resolved(Axis::auto(), Some(Range::new(5.0, 5.0)));
        assert!(!axis.range().is_degenerate());
        assert!(axis.range().contains(5.0));
    }

    #[test]
    fn a_log_axis_is_lifted_off_zero() {
        let axis = resolved(
            Axis::auto().with_scale(Scale::Log10),
            Some(Range::new(0.0, 100.0)),
        );
        assert!(axis.range().min > 0.0, "got {:?}", axis.range());
    }

    #[test]
    fn a_zoomed_view_wins_over_the_bounds() {
        let mut axis = Axis::fixed(0.0, 100.0);
        axis.resolve(None, Some(Range::new(10.0, 20.0)));
        assert_eq!(axis.range(), Range::new(10.0, 20.0));
    }

    #[test]
    fn a_fixed_axis_does_not_need_the_data() {
        assert!(!Bounds::Fixed(Range::new(0.0, 1.0)).needs_data());
        assert!(Bounds::Auto { padding: 0.0 }.needs_data());
        assert!(Bounds::Window { span: 10.0 }.needs_data());
    }

    #[test]
    fn ticks_are_made_once_while_the_range_holds() {
        let mut axis = Axis::fixed(0.0, 10.0);
        assert!(axis.resolved_ticks().is_empty(), "none until resolved");
        axis.resolve(None, None);
        let first = axis.resolved_ticks().as_ptr();
        assert!(!axis.resolved_ticks().is_empty());
        axis.resolve(None, None);
        assert_eq!(axis.resolved_ticks().as_ptr(), first, "kept, not remade");
        axis.resolve(None, Some(Range::new(0.0, 5.0)));
        assert!(axis.resolved_ticks().iter().all(|t| t.value <= 5.0));
    }
}
