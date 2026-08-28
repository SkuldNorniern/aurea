//! One-call constructors for the graphs people actually ask for.
//!
//! The full API is there for anything unusual, but most of the time you want a
//! line chart, a live feed, a sparkline or a scope, and having to assemble one
//! out of an axis, a style, a series and a tick plan every time is busywork.
//! Each of these returns a plain [`Graph`] or [`Scope`], so anything can be
//! changed afterwards.

use aurea_render::Color;

use super::plot::{Axis, Graph};
use super::scope::{Channel, Scope, Timebase, Trigger};
use super::series::{Plot, Series};
use super::style::{GraphStyle, Margin};

/// A line chart over explicit points.
///
/// ```rust
/// use aurea::render::graph::quick;
///
/// let graph = quick::line("temperature", vec![(0.0, 20.0), (1.0, 21.5)]);
/// assert_eq!(graph.series.len(), 1);
/// ```
pub fn line(name: impl Into<String>, points: Vec<(f64, f64)>) -> Graph {
    let mut graph = Graph::new();
    graph.add_series(Series::xy(name, points));
    graph
}

/// A line chart with a mark at every point, for data with few enough samples
/// that the individual readings matter.
pub fn scatter(name: impl Into<String>, points: Vec<(f64, f64)>) -> Graph {
    let mut graph = Graph::new();
    graph.add_series(
        Series::xy(name, points)
            .with_plot(Plot::Points)
            .with_points_shown(true),
    );
    graph
}

/// A filled area chart down to zero.
pub fn area(name: impl Into<String>, points: Vec<(f64, f64)>) -> Graph {
    let mut graph = Graph::new();
    graph.add_series(Series::xy(name, points).with_plot(Plot::Area));
    graph
}

/// Bars from zero to each point.
pub fn bars(name: impl Into<String>, points: Vec<(f64, f64)>) -> Graph {
    let mut graph = Graph::new();
    graph.add_series(Series::xy(name, points).with_plot(Plot::Bars));
    graph
}

/// A live feed that scrolls: the x axis keeps a window on the newest samples
/// rather than squashing more and more data into the same width.
///
/// Feed it with [`Graph::push`] and the series index this returns.
///
/// ```rust
/// use aurea::render::graph::quick;
///
/// let (mut graph, signal) = quick::live("cpu", 600);
/// graph.push(signal, 0.42);
/// ```
pub fn live(name: impl Into<String>, capacity: usize) -> (Graph, usize) {
    let mut graph = Graph::new();
    // One sample per x unit, so the window is a sample count.
    let span = super::numeric::count_to_f64(capacity.max(1));
    graph.x = Axis::window(span);
    let index = graph.add_series(Series::rolling(name, capacity));
    (graph, index)
}

/// Several live feeds sharing one plot, in palette order.
///
/// Returns the series indices in the order the names were given.
pub fn live_multi(names: &[&str], capacity: usize) -> (Graph, Vec<usize>) {
    let mut graph = Graph::new();
    graph.x = Axis::window(super::numeric::count_to_f64(capacity.max(1)));
    let indices = names
        .iter()
        .map(|name| graph.add_series(Series::rolling(*name, capacity)))
        .collect();
    (graph, indices)
}

/// A bare trace with no grid, axes or background, for a chart that sits inline
/// in a row of text or a table cell.
pub fn sparkline(values: &[f64]) -> Graph {
    let points = values
        .iter()
        .enumerate()
        .map(|(i, v)| (super::numeric::count_to_f64(i), *v))
        .collect();
    let mut graph = Graph::new().with_style(GraphStyle {
        margin: Margin::uniform(2.0),
        ..GraphStyle::bare()
    });
    graph.add_series(Series::xy("", points).with_width(1.0));
    graph
}

/// An oscilloscope with `channels` inputs named CH1, CH2 and so on, triggered
/// on a rising edge through zero.
///
/// ```rust
/// use aurea::render::graph::quick;
///
/// let mut scope = quick::scope(2, 20_000.0);
/// scope.push(0, 0.5);
/// ```
pub fn scope(channels: usize, sample_rate: f64) -> Scope {
    // Room for several sweeps, so the trigger has somewhere to search.
    let capacity = window_capacity(sample_rate);
    let mut scope = Scope::new(capacity);
    scope.timebase = Timebase::new(0.001, sample_rate);
    scope.trigger = Trigger::rising(0.0);

    for i in 0..channels {
        let color = scope.style.palette_color(i);
        scope.add_channel(
            Channel::with_capacity(format!("CH{}", i + 1), capacity).with_color(color),
        );
    }
    scope
}

/// A single-channel scope in one colour.
pub fn scope_single(name: impl Into<String>, sample_rate: f64, color: Color) -> Scope {
    let capacity = window_capacity(sample_rate);
    let mut scope = Scope::new(capacity);
    scope.timebase = Timebase::new(0.001, sample_rate);
    scope.trigger = Trigger::rising(0.0);
    scope.add_channel(Channel::with_capacity(name, capacity).with_color(color));
    scope
}

/// Samples to keep per channel: a few sweeps at the default timebase, bounded
/// so a high sample rate does not ask for an enormous buffer.
fn window_capacity(sample_rate: f64) -> usize {
    let per_sweep = sample_rate * 0.01;
    if !per_sweep.is_finite() || per_sweep <= 0.0 {
        return 4096;
    }
    super::numeric::f64_to_count(per_sweep.clamp(1024.0, 262_144.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_holds_the_points_it_was_given() {
        let graph = line("s", vec![(0.0, 1.0), (1.0, 2.0)]);
        assert_eq!(graph.series.len(), 1);
        assert_eq!(graph.series[0].points.len(), 2);
    }

    #[test]
    fn scatter_shows_its_points() {
        let graph = scatter("s", vec![(0.0, 1.0)]);
        assert_eq!(graph.series[0].plot, Plot::Points);
        assert!(graph.series[0].show_points);
    }

    #[test]
    fn area_and_bars_pick_their_plot() {
        assert_eq!(area("s", vec![(0.0, 1.0)]).series[0].plot, Plot::Area);
        assert_eq!(bars("s", vec![(0.0, 1.0)]).series[0].plot, Plot::Bars);
    }

    #[test]
    fn a_live_graph_scrolls_instead_of_squashing() {
        let (graph, _) = live("s", 100);
        assert!(
            matches!(graph.x.bounds, super::super::Bounds::Window { .. }),
            "a live feed should keep a window on the newest samples"
        );
    }

    #[test]
    fn a_live_graph_takes_samples_at_the_index_it_returned() {
        let (mut graph, index) = live("s", 8);
        graph.push(index, 5.0);
        assert_eq!(graph.series[index].points.len(), 1);
    }

    #[test]
    fn live_multi_returns_one_index_per_name() {
        let (graph, indices) = live_multi(&["a", "b", "c"], 16);
        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(graph.series.len(), 3);
    }

    #[test]
    fn a_zero_capacity_live_graph_does_not_make_a_zero_window() {
        let (graph, _) = live("s", 0);
        match graph.x.bounds {
            super::super::Bounds::Window { span } => assert!(span > 0.0),
            other => panic!("expected a window, got {other:?}"),
        }
    }

    #[test]
    fn a_sparkline_has_no_furniture() {
        let graph = sparkline(&[1.0, 2.0, 3.0]);
        assert!(graph.style.background.is_none());
        assert!(!graph.style.grid.show_horizontal);
        assert_eq!(graph.series[0].points.len(), 3);
    }

    #[test]
    fn an_empty_sparkline_is_fine() {
        assert_eq!(sparkline(&[]).series[0].points.len(), 0);
    }

    #[test]
    fn a_scope_gets_named_channels_in_palette_order() {
        let scope = scope(3, 20_000.0);
        let names: Vec<&str> = scope.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["CH1", "CH2", "CH3"]);
        assert!(scope.channels.iter().all(|c| c.color.is_some()));
    }

    #[test]
    fn a_scope_is_triggered_by_default() {
        assert!(scope(1, 20_000.0).trigger.enabled);
    }

    #[test]
    fn a_scope_channel_holds_more_than_one_sweep() {
        let scope = scope(1, 20_000.0);
        let per_sweep = scope.timebase.window_samples();
        assert!(
            scope.channels[0].samples.capacity() > per_sweep,
            "the trigger needs history to search"
        );
    }

    #[test]
    fn a_silly_sample_rate_still_gives_a_usable_buffer() {
        assert!(window_capacity(0.0) > 0);
        assert!(window_capacity(f64::NAN) > 0);
        assert!(window_capacity(1e12) <= 262_144);
    }
}
