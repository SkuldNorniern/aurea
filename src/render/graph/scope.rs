//! An oscilloscope view.
//!
//! A [`Scope`] is a [`Graph`] set up the way an instrument is. Channels carry
//! their own volts per division and vertical offset, the timebase is seconds
//! per division rather than an x range, and a trigger picks where in the
//! captured samples the drawn window starts so a repeating signal stands still
//! instead of sliding across the screen.

use aurea_foundation::AureaResult;
use aurea_render::{Color, DrawingContext, Paint, PaintStyle, Point, Rect};

use super::buffer::SampleBuffer;
use super::numeric::count_to_f64;
use super::plot::{Axis, Cursor, Graph};
use super::scale::Range;
use super::series::{Points, Series};
use super::style::GraphStyle;
use super::ticks::TickPlan;

/// Which way the signal has to cross the level to fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerEdge {
    /// Below the level, then at or above it.
    #[default]
    Rising,
    /// Above the level, then at or below it.
    Falling,
    /// Either direction.
    Either,
}

/// What to draw when the trigger does not fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerMode {
    /// Draw the newest samples anyway, so a dead input still shows a trace.
    #[default]
    Auto,
    /// Hold the last triggered capture. A signal that stops leaves the last
    /// good sweep on screen instead of a rolling mess.
    Normal,
    /// Capture once, then stop.
    Single,
}

/// When to start the drawn window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trigger {
    pub enabled: bool,
    /// The value the signal has to cross.
    pub level: f64,
    pub edge: TriggerEdge,
    pub mode: TriggerMode,
    /// Which channel is watched, by index.
    pub source: usize,
    /// Fraction of the window shown before the trigger point, 0 to 1. At 0.5
    /// the crossing sits in the middle and you can see what led up to it.
    pub position: f64,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            enabled: false,
            level: 0.0,
            edge: TriggerEdge::Rising,
            mode: TriggerMode::Auto,
            source: 0,
            position: 0.5,
        }
    }
}

impl Trigger {
    /// Fires on a rising crossing of `level`.
    pub fn rising(level: f64) -> Self {
        Self {
            enabled: true,
            level,
            edge: TriggerEdge::Rising,
            ..Self::default()
        }
    }

    /// Fires on a falling crossing of `level`.
    pub fn falling(level: f64) -> Self {
        Self {
            enabled: true,
            level,
            edge: TriggerEdge::Falling,
            ..Self::default()
        }
    }

    pub fn with_mode(mut self, mode: TriggerMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_source(mut self, source: usize) -> Self {
        self.source = source;
        self
    }

    /// Where the trigger point sits across the window, 0 to 1.
    pub fn with_position(mut self, position: f64) -> Self {
        self.position = position.clamp(0.0, 1.0);
        self
    }

    /// Whether `previous` to `current` crosses the level the right way.
    fn fires(&self, previous: f64, current: f64) -> bool {
        let rising = previous < self.level && current >= self.level;
        let falling = previous > self.level && current <= self.level;
        match self.edge {
            TriggerEdge::Rising => rising,
            TriggerEdge::Falling => falling,
            TriggerEdge::Either => rising || falling,
        }
    }
}

/// How much time the screen covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timebase {
    /// Seconds across one horizontal division.
    pub seconds_per_division: f64,
    /// How many divisions across.
    pub divisions: usize,
    /// How often samples arrive. Turns sample counts into seconds.
    pub sample_rate: f64,
}

impl Default for Timebase {
    fn default() -> Self {
        Self {
            seconds_per_division: 0.001,
            divisions: 10,
            sample_rate: 48_000.0,
        }
    }
}

impl Timebase {
    /// A timebase at `seconds_per_division`, sampling at `sample_rate`.
    pub fn new(seconds_per_division: f64, sample_rate: f64) -> Self {
        Self {
            seconds_per_division,
            sample_rate,
            ..Self::default()
        }
    }

    /// Seconds across the whole screen.
    pub fn window_seconds(&self) -> f64 {
        self.seconds_per_division * count_to_f64(self.divisions)
    }

    /// How many samples fill the screen, at least one.
    pub fn window_samples(&self) -> usize {
        let samples = self.window_seconds() * self.sample_rate;
        if !samples.is_finite() || samples <= 1.0 {
            return 1;
        }
        super::numeric::f64_to_count(samples.min(1e9))
    }

    /// Seconds between one sample and the next.
    pub fn sample_interval(&self) -> f64 {
        if self.sample_rate <= 0.0 || !self.sample_rate.is_finite() {
            return 0.0;
        }
        1.0 / self.sample_rate
    }
}

/// One input.
#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    /// Everything captured, not just what fits on screen. The trigger searches
    /// this, so it has to hold more than one window.
    pub samples: SampleBuffer,
    /// Value across one vertical division.
    pub volts_per_division: f64,
    /// Shifts the trace up or down, in volts.
    pub offset: f64,
    pub color: Option<Color>,
    pub visible: bool,
}

impl Channel {
    /// A channel holding `capacity` samples.
    pub fn with_capacity(name: impl Into<String>, capacity: usize) -> Self {
        Self {
            name: name.into(),
            samples: SampleBuffer::with_capacity(capacity),
            volts_per_division: 1.0,
            offset: 0.0,
            color: None,
            visible: true,
        }
    }

    /// A channel with room for a few screens of samples.
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_capacity(name, 4096)
    }

    pub fn with_volts_per_division(mut self, volts: f64) -> Self {
        self.volts_per_division = volts;
        self
    }

    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Adds one sample.
    pub fn push(&mut self, value: f64) {
        self.samples.push(value);
    }

    /// Adds many samples in order.
    pub fn extend(&mut self, values: impl IntoIterator<Item = f64>) {
        self.samples.extend(values);
    }
}

/// An oscilloscope over one or more channels.
pub struct Scope {
    pub channels: Vec<Channel>,
    pub timebase: Timebase,
    pub trigger: Trigger,
    pub style: GraphStyle,
    /// Divisions above and below the centre line.
    pub vertical_divisions: usize,
    /// Where the last trigger fired, as an index into the capture. Held so
    /// [`TriggerMode::Normal`] can keep showing the last good sweep.
    last_trigger: Option<usize>,
    /// The samples a finished single shot caught, one run per channel.
    ///
    /// Kept rather than re-read, because an index into the ring does not name
    /// a fixed sample: new data moves the contents under it, so a sweep held
    /// by index turns into a different sweep once the ring wraps.
    frozen: Option<Vec<Vec<f64>>>,
    /// Cleared by [`Self::arm`], set once a single-shot capture has fired.
    single_done: bool,
    /// Rebuilt each draw from the channels.
    graph: Graph,
    /// Capacity handed to channels made by [`Scope::add_input`].
    default_capacity: usize,
}

impl Scope {
    /// A scope whose channels each hold `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        let mut style = GraphStyle::dark();
        // A scope graticule is even divisions, not chosen round numbers, and
        // the labels are volts and seconds rather than sample indices.
        style.grid.zero = Some(style.grid.major);

        Self {
            channels: Vec::new(),
            timebase: Timebase::default(),
            trigger: Trigger::default(),
            style,
            vertical_divisions: 4,
            last_trigger: None,
            frozen: None,
            single_done: false,
            graph: Graph::new(),
            default_capacity: capacity,
        }
    }

    /// Adds a channel named `name` holding the scope's default capacity, and
    /// hands back its index. The short way to get an input going.
    pub fn add_input(&mut self, name: impl Into<String>) -> usize {
        let capacity = self.default_capacity;
        self.add_channel(Channel::with_capacity(name, capacity))
    }

    /// Adds a channel and hands back its index.
    pub fn add_channel(&mut self, channel: Channel) -> usize {
        self.channels.push(channel);
        self.channels.len() - 1
    }

    /// The channel at `index`.
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut Channel> {
        self.channels.get_mut(index)
    }

    /// Adds a sample to the channel at `index`.
    pub fn push(&mut self, index: usize, value: f64) {
        if let Some(channel) = self.channels.get_mut(index) {
            channel.push(value);
        }
    }

    /// Clears every channel and forgets the last trigger.
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            channel.samples.clear();
        }
        self.last_trigger = None;
        self.frozen = None;
    }

    /// Re-arms a single-shot capture.
    pub fn arm(&mut self) {
        self.single_done = false;
        self.last_trigger = None;
        self.frozen = None;
    }

    /// Copies the sweep each channel is showing and stops on it.
    ///
    /// Called when a single shot fires. Taking a copy is the whole point: the
    /// ring keeps filling, and anything identifying the sweep by position in
    /// it stops meaning the same samples once it wraps.
    fn freeze_sweep(&mut self, window: usize) {
        self.single_done = true;
        self.frozen = Some(
            self.channels
                .iter()
                .map(|channel| {
                    let (start, take) = self.window_for(channel, window);
                    let (head, tail) = channel.samples.range(start, take);
                    head.iter().chain(tail).copied().collect()
                })
                .collect(),
        );
    }

    /// The samples channel `index` is showing: the frozen sweep if a single
    /// shot caught one, otherwise whatever the live window holds.
    ///
    /// `window` is the sweep length in samples, as
    /// [`Timebase::window_samples`] gives it. Useful for reading out or
    /// exporting a capture; drawing does not go through here, because this
    /// allocates and the draw path works from the buffer's own slices.
    pub fn sweep_samples(&self, index: usize, window: usize) -> Vec<f64> {
        if let Some(frozen) = &self.frozen {
            return frozen.get(index).cloned().unwrap_or_default();
        }
        let Some(channel) = self.channels.get(index) else {
            return Vec::new();
        };
        let (start, take) = self.window_for(channel, window);
        let (head, tail) = channel.samples.range(start, take);
        head.iter().chain(tail).copied().collect()
    }

    /// Whether a single-shot capture has fired and stopped.
    pub fn is_stopped(&self) -> bool {
        self.trigger.mode == TriggerMode::Single && self.single_done
    }

    /// The value range one division covers, over all visible channels.
    ///
    /// Channels share the screen, so the range has to hold the one that needs
    /// the most room.
    fn vertical_range(&self) -> Range {
        let divisions = count_to_f64(self.vertical_divisions.max(1));
        let mut half = 0.0f64;
        for channel in self.channels.iter().filter(|c| c.visible) {
            half = half.max(channel.volts_per_division.abs() * divisions);
        }
        if half <= 0.0 {
            half = 1.0;
        }
        Range::new(-half, half)
    }

    /// Where the drawn window starts in a channel's capture.
    ///
    /// Searches back from the newest sample for a crossing, so the most recent
    /// sweep is the one shown. At [`Trigger::position`] 0 the window opens on
    /// the crossing sample; higher positions open earlier, so the run-up to the
    /// crossing is on screen. `None` means no crossing was found.
    fn find_trigger(&self, channel: &Channel, window: usize) -> Option<usize> {
        if !self.trigger.enabled || channel.samples.len() < 2 {
            return None;
        }
        let len = channel.samples.len();
        // Leave room for what is shown before and after the crossing.
        let before = super::numeric::f64_to_count(
            count_to_f64(window) * self.trigger.position.clamp(0.0, 1.0),
        );
        let after = window.saturating_sub(before);

        // The newest sample that can still be the crossing. Capped at the last
        // real index: at position 1.0 the whole window sits before the trigger,
        // so `after` is zero and this would otherwise start one past the end —
        // where the first read failed and abandoned the search entirely.
        let newest_usable = len.saturating_sub(after).min(len - 1);
        let mut index = newest_usable;
        while index > before.max(1) {
            let (Some(previous), Some(current)) =
                (channel.samples.get(index - 1), channel.samples.get(index))
            else {
                // A gap says nothing about the rest of the capture, so keep
                // looking rather than giving up on the sweep.
                index -= 1;
                continue;
            };
            if self.trigger.fires(previous, current) {
                return Some(index - before);
            }
            index -= 1;
        }
        None
    }

    /// The samples to draw for a channel, and the index they start at.
    fn window_for(&self, channel: &Channel, window: usize) -> (usize, usize) {
        let len = channel.samples.len();
        let take = window.min(len);

        // A finished single shot shows the sweep it caught and nothing else.
        // Searching again each frame let newer samples slide the trace along
        // while `is_stopped` reported it frozen.
        if self.is_stopped() {
            let start = self.last_trigger.unwrap_or(0);
            return (start.min(len.saturating_sub(take)), take);
        }

        match self.find_trigger(channel, window) {
            Some(start) => (start.min(len.saturating_sub(take)), take),
            None => match self.trigger.mode {
                // No crossing: Auto shows the newest samples anyway.
                TriggerMode::Auto | TriggerMode::Single => (len.saturating_sub(take), take),
                // Normal holds the previous sweep rather than rolling.
                TriggerMode::Normal => match self.last_trigger {
                    Some(start) => (start.min(len.saturating_sub(take)), take),
                    None => (len.saturating_sub(take), take),
                },
            },
        }
    }

    /// Builds the plot for the current capture and draws it into `area`.
    pub fn draw(&mut self, ctx: &mut dyn DrawingContext, area: Rect) -> AureaResult<()> {
        let window = self.timebase.window_samples();
        let interval = self.timebase.sample_interval();
        let vertical = self.vertical_range();
        // How many pixel columns the trace has to fit into. More samples than
        // that cannot be told apart, so they are collapsed to an envelope
        // before anything is built.
        let columns = super::numeric::f64_to_count(f64::from(area.width.max(1.0)));

        self.graph.style = self.style.clone();
        // A graticule is ruled in even divisions, not at round numbers, and
        // reads in engineering units the way an instrument front panel does.
        self.graph.x = Axis::fixed(0.0, self.timebase.window_seconds()).with_ticks(
            TickPlan::default()
                .with_even_divisions(self.timebase.divisions)
                .with_engineering(true)
                .with_suffix("s"),
        );
        self.graph.y = Axis::fixed(vertical.min, vertical.max).with_ticks(
            TickPlan::default()
                .with_even_divisions(self.vertical_divisions * 2)
                .with_engineering(true)
                .with_suffix("V"),
        );

        // A line at the trigger level, so it is clear what the sweep is
        // waiting for and which channel it is watching.
        self.graph.cursors.clear();
        if self.trigger.enabled {
            let color = self
                .channels
                .get(self.trigger.source)
                .and_then(|c| c.color)
                .unwrap_or(Color::rgb(255, 210, 120));
            self.graph
                .cursors
                .push(Cursor::horizontal(self.trigger.level, dim(color, 160)).with_label("T"));
        }

        self.graph.series.clear();
        let mut fired = self.last_trigger;

        for (index, channel) in self.channels.iter().enumerate() {
            if !channel.visible {
                continue;
            }
            let (start, take) = self.window_for(channel, window);
            // A stopped single shot keeps the sweep it has; looking for a newer
            // crossing is exactly what it is stopped from doing.
            if index == self.trigger.source && self.trigger.enabled && !self.is_stopped() {
                fired = self.find_trigger(channel, window).or(fired);
            }

            // Offset shifts the trace on screen without touching the captured
            // sample, the way a scope front panel does — so it is applied here
            // and still works on a sweep that was frozen earlier.
            let points = match &self.frozen {
                Some(frozen) => sweep_points_from(
                    frozen.get(index).map_or(&[][..], Vec::as_slice),
                    &[],
                    channel.offset,
                    interval,
                    columns,
                ),
                None => sweep_points(channel, start, take, interval, columns),
            };

            let mut series = Series::new(channel.name.clone(), Points::Xy(points));
            series.color = channel.color;
            series.width = 1.25;
            self.graph.series.push(series);
        }

        self.last_trigger = fired;
        if self.trigger.mode == TriggerMode::Single && fired.is_some() && !self.single_done {
            // Take the copy now, on the frame it fired. Waiting would mean
            // capturing whatever the ring held by then instead.
            self.freeze_sweep(window);
        }

        self.graph.draw(ctx, area)?;
        self.draw_readout(ctx)
    }

    /// The line of text along the top: timebase, then each visible channel with
    /// what one division is worth. Same information a front panel shows.
    ///
    /// It goes in the top margin rather than over the graticule, so there has
    /// to be room for it; with no margin there is nowhere to put it.
    fn draw_readout(&self, ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        let area = self.graph.plot_area();
        if area.width <= 0.0 || area.height <= 0.0 || self.style.margin.top < 12.0 {
            return Ok(());
        }

        let font = self.style.x_axis.label_font.clone();
        let mut x = area.x;
        let y = area.y - 6.0;

        let (per_div, prefix) = engineering_value(self.timebase.seconds_per_division);
        let timebase = format!("{per_div:.0}{prefix}s/div");
        let paint = Paint::new()
            .color(self.style.x_axis.label_color)
            .style(PaintStyle::Fill);
        ctx.draw_text_with_font(&timebase, Point::new(x, y), &font, &paint)?;
        x += ctx.measure_text(&timebase, &font)?.width + 12.0;

        for (index, channel) in self.channels.iter().enumerate() {
            if !channel.visible {
                continue;
            }
            let (volts, prefix) = engineering_value(channel.volts_per_division);
            let text = format!("{}: {volts:.0}{prefix}V/div", channel.name);
            let color = channel
                .color
                .unwrap_or_else(|| self.style.palette_color(index));
            let paint = Paint::new().color(color).style(PaintStyle::Fill);
            ctx.draw_text_with_font(&text, Point::new(x, y), &font, &paint)?;
            x += ctx.measure_text(&text, &font)?.width + 12.0;
        }
        Ok(())
    }

    /// The plot behind the scope, for anything the scope API does not cover.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
}

/// A colour at a different opacity, for a marker that should not compete with
/// the trace.
fn dim(color: Color, alpha: u8) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// A value in its engineering range, with the prefix. Shared with the tick
/// labels so the readout and the axis agree on units.
fn engineering_value(value: f64) -> (f64, &'static str) {
    const STEPS: [(f64, &str); 7] = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "µ"),
        (1e-9, "n"),
    ];
    let magnitude = value.abs();
    if magnitude == 0.0 || !magnitude.is_finite() {
        return (0.0, "");
    }
    for (factor, prefix) in STEPS {
        if magnitude >= factor {
            return (value / factor, prefix);
        }
    }
    (value / 1e-9, "n")
}

/// The points for one sweep, collapsed to a per-column envelope when there are
/// more samples than pixels.
///
/// Without this the cost of a frame grows with the capture length rather than
/// with the size of the screen: a 100k-sample sweep built 100k pairs to draw
/// into 900 columns.
fn sweep_points(
    channel: &Channel,
    start: usize,
    take: usize,
    interval: f64,
    columns: usize,
) -> Vec<(f64, f64)> {
    // Walk contiguous slices rather than indexing: reading through `get` costs
    // two integer divisions per sample, which dominates a long sweep.
    let (head, tail) = channel.samples.range(start, take);
    sweep_points_from(head, tail, channel.offset, interval, columns)
}

/// The same, over samples already in hand — a frozen sweep has no ring to
/// read, so it arrives as one plain run.
fn sweep_points_from(
    head: &[f64],
    tail: &[f64],
    offset: f64,
    interval: f64,
    columns: usize,
) -> Vec<(f64, f64)> {
    let take = head.len() + tail.len();
    let budget = columns.saturating_mul(2).max(64);

    if take <= budget {
        return head
            .iter()
            .chain(tail)
            .enumerate()
            .map(|(i, value)| (interval * count_to_f64(i), value + offset))
            .collect();
    }

    let mut points = Vec::with_capacity(budget);
    let total = head.len() + tail.len();
    let per_column = count_to_f64(total) / count_to_f64(columns.max(1));

    let mut column = 0usize;
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    let mut column_start = 0usize;

    for (index, value) in head.iter().chain(tail).copied().enumerate() {
        let this_column = super::numeric::f64_to_count(count_to_f64(index) / per_column);
        if this_column != column {
            if low.is_finite() {
                push_envelope(&mut points, interval, column_start, low, high, offset);
            }
            column = this_column;
            column_start = index;
            low = f64::INFINITY;
            high = f64::NEG_INFINITY;
        }
        low = low.min(value);
        high = high.max(value);
    }
    if low.is_finite() {
        push_envelope(&mut points, interval, column_start, low, high, offset);
    }
    points
}

/// One column of the envelope, drawn bottom to top.
fn push_envelope(
    points: &mut Vec<(f64, f64)>,
    interval: f64,
    index: usize,
    low: f64,
    high: f64,
    offset: f64,
) {
    let t = interval * count_to_f64(index);
    points.push((t, low + offset));
    if (high - low).abs() > f64::EPSILON {
        points.push((t, high + offset));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_with(samples: &[f64]) -> Scope {
        let mut scope = Scope::new(1024);
        let mut channel = Channel::with_capacity("CH1", 1024);
        channel.extend(samples.iter().copied());
        scope.add_channel(channel);
        scope
    }

    #[test]
    fn a_rising_trigger_fires_on_an_upward_crossing() {
        let trigger = Trigger::rising(0.0);
        assert!(trigger.fires(-1.0, 1.0));
        assert!(!trigger.fires(1.0, -1.0));
    }

    #[test]
    fn a_falling_trigger_fires_on_a_downward_crossing() {
        let trigger = Trigger::falling(0.0);
        assert!(trigger.fires(1.0, -1.0));
        assert!(!trigger.fires(-1.0, 1.0));
    }

    #[test]
    fn an_either_trigger_fires_both_ways() {
        let trigger = Trigger {
            edge: TriggerEdge::Either,
            ..Trigger::rising(0.0)
        };
        assert!(trigger.fires(-1.0, 1.0));
        assert!(trigger.fires(1.0, -1.0));
    }

    #[test]
    fn a_trigger_at_a_level_the_signal_never_reaches_does_not_fire() {
        let trigger = Trigger::rising(10.0);
        assert!(!trigger.fires(-1.0, 1.0));
    }

    #[test]
    fn the_window_covers_the_timebase() {
        let timebase = Timebase::new(0.001, 1000.0);
        assert!((timebase.window_seconds() - 0.01).abs() < 1e-12);
        assert_eq!(timebase.window_samples(), 10);
    }

    #[test]
    fn a_bad_sample_rate_gives_no_interval() {
        assert_eq!(Timebase::new(0.001, 0.0).sample_interval(), 0.0);
        assert_eq!(Timebase::new(0.001, -5.0).sample_interval(), 0.0);
    }

    #[test]
    fn a_window_is_never_zero_samples() {
        assert_eq!(Timebase::new(0.0, 1000.0).window_samples(), 1);
    }

    #[test]
    fn the_vertical_range_holds_the_widest_channel() {
        let mut scope = Scope::new(64);
        scope.vertical_divisions = 4;
        scope.add_channel(Channel::new("a").with_volts_per_division(1.0));
        scope.add_channel(Channel::new("b").with_volts_per_division(2.0));

        assert_eq!(scope.vertical_range(), Range::new(-8.0, 8.0));
    }

    #[test]
    fn a_scope_with_no_channels_still_has_a_range() {
        let scope = Scope::new(64);
        assert!(!scope.vertical_range().is_degenerate());
    }

    #[test]
    fn a_hidden_channel_does_not_widen_the_range() {
        let mut scope = Scope::new(64);
        scope.vertical_divisions = 1;
        scope.add_channel(Channel::new("a").with_volts_per_division(1.0));
        scope.add_channel(
            Channel::new("b")
                .with_volts_per_division(100.0)
                .with_visible(false),
        );

        assert_eq!(scope.vertical_range(), Range::new(-1.0, 1.0));
    }

    #[test]
    fn the_trigger_lines_the_window_up_with_a_crossing() {
        // Square wave: crossings every 4 samples.
        let samples: Vec<f64> = (0..64)
            .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
            .collect();
        let mut scope = scope_with(&samples);
        scope.trigger = Trigger::rising(0.0).with_position(0.0);

        // At position 0 the window opens on the crossing sample itself, so the
        // one before it is the last sample under the level.
        let start = scope
            .find_trigger(&scope.channels[0], 8)
            .expect("a square wave has crossings");
        let previous = scope.channels[0].samples.get(start - 1).expect("in range");
        let current = scope.channels[0].samples.get(start).expect("in range");
        assert!(
            previous < 0.0 && current >= 0.0,
            "window should open on a rising edge, got {previous} then {current}"
        );
    }

    /// Position 1.0 puts the whole window before the crossing, which left the
    /// search starting one sample past the end and giving up immediately.
    #[test]
    fn a_trigger_at_the_far_edge_still_fires() {
        let samples: Vec<f64> = (0..64)
            .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
            .collect();
        let mut scope = scope_with(&samples);
        scope.trigger = Trigger::rising(0.0).with_position(1.0);

        assert!(
            scope.find_trigger(&scope.channels[0], 8).is_some(),
            "a square wave has crossings at any trigger position"
        );
    }

    #[test]
    fn every_trigger_position_finds_a_crossing() {
        let samples: Vec<f64> = (0..64)
            .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
            .collect();

        for tenth in 0..=10 {
            let mut scope = scope_with(&samples);
            let position = f64::from(tenth) / 10.0;
            scope.trigger = Trigger::rising(0.0).with_position(position);

            assert!(
                scope.find_trigger(&scope.channels[0], 8).is_some(),
                "no crossing found at position {position}"
            );
        }
    }

    /// A finished single shot holds the sweep it caught. It used to keep
    /// searching, so the trace slid along while `is_stopped` said otherwise.
    #[test]
    fn a_stopped_single_shot_holds_its_sweep() {
        let samples: Vec<f64> = (0..64)
            .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
            .collect();
        let mut scope = scope_with(&samples);
        scope.trigger = Trigger::rising(0.0).with_mode(TriggerMode::Single);

        // Pretend a sweep was captured at a known place.
        scope.last_trigger = Some(12);
        scope.single_done = true;
        assert!(scope.is_stopped());

        let before = scope.window_for(&scope.channels[0], 8);

        // More samples arrive; the frozen sweep must not move.
        for value in 0..32 {
            scope.push(0, f64::from(value % 2) * 2.0 - 1.0);
        }
        let after = scope.window_for(&scope.channels[0], 8);

        assert_eq!(before, after, "a stopped sweep should not follow new data");
    }

    /// Holding the same *indices* is not holding the same sweep: the ring
    /// moves under them, so after enough new data those indices name different
    /// samples and the frozen trace quietly becomes something else.
    #[test]
    fn a_stopped_single_shot_holds_the_same_samples() {
        // A ring only just big enough to hold the capture, so the pushes
        // below genuinely overwrite it. With a roomy buffer nothing wraps and
        // this passes whether or not the sweep is really held.
        let mut scope = Scope::new(64);
        let mut channel = Channel::with_capacity("CH1", 64);
        channel.extend((0..64).map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 }));
        scope.add_channel(channel);
        scope.trigger = Trigger::rising(0.0).with_mode(TriggerMode::Single);
        scope.last_trigger = Some(12);
        scope.freeze_sweep(8);

        let before = scope.sweep_samples(0, 8);
        assert!(!before.is_empty());

        // Enough new data to overwrite the whole ring several times.
        for i in 0..300 {
            scope.push(0, f64::from(i % 2) * 8.0 + 40.0);
        }

        assert_eq!(
            scope.sweep_samples(0, 8),
            before,
            "the captured sweep changed after the ring wrapped"
        );
    }

    #[test]
    fn arming_lets_a_single_shot_track_again() {
        let samples: Vec<f64> = (0..64)
            .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
            .collect();
        let mut scope = scope_with(&samples);
        scope.trigger = Trigger::rising(0.0).with_mode(TriggerMode::Single);
        scope.last_trigger = Some(12);
        scope.single_done = true;

        scope.arm();

        assert!(!scope.is_stopped());
        assert!(
            scope.find_trigger(&scope.channels[0], 8).is_some(),
            "an armed scope searches again"
        );
    }

    #[test]
    fn a_flat_signal_never_triggers() {
        let mut scope = scope_with(&[0.5; 64]);
        scope.trigger = Trigger::rising(0.0);

        assert_eq!(scope.find_trigger(&scope.channels[0], 8), None);
    }

    #[test]
    fn auto_mode_shows_the_newest_samples_when_nothing_triggers() {
        let samples: Vec<f64> = (0..64).map(f64::from).collect();
        let mut scope = scope_with(&samples);
        scope.trigger = Trigger::rising(1000.0).with_mode(TriggerMode::Auto);

        let (start, take) = scope.window_for(&scope.channels[0], 8);
        assert_eq!(take, 8);
        assert_eq!(start, 56, "should be the last 8 samples");
    }

    #[test]
    fn a_disabled_trigger_never_fires() {
        let samples: Vec<f64> = (0..64)
            .map(|i| if i % 2 == 0 { -1.0 } else { 1.0 })
            .collect();
        let scope = scope_with(&samples);

        assert!(!scope.trigger.enabled);
        assert_eq!(scope.find_trigger(&scope.channels[0], 8), None);
    }

    #[test]
    fn a_window_wider_than_the_capture_is_clamped() {
        let mut scope = scope_with(&[1.0, 2.0, 3.0]);
        scope.trigger.enabled = false;

        let (start, take) = scope.window_for(&scope.channels[0], 100);
        assert_eq!((start, take), (0, 3));
    }

    #[test]
    fn trigger_position_is_clamped() {
        assert_eq!(Trigger::rising(0.0).with_position(5.0).position, 1.0);
        assert_eq!(Trigger::rising(0.0).with_position(-1.0).position, 0.0);
    }

    #[test]
    fn arming_clears_a_finished_single_shot() {
        let mut scope = Scope::new(16);
        scope.trigger = Trigger::rising(0.0).with_mode(TriggerMode::Single);
        scope.single_done = true;
        assert!(scope.is_stopped());

        scope.arm();
        assert!(!scope.is_stopped());
    }

    #[test]
    fn pushing_to_a_missing_channel_is_ignored() {
        let mut scope = Scope::new(16);
        scope.push(3, 1.0);
        assert!(scope.channels.is_empty());
    }
}
