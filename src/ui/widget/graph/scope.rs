//! An oscilloscope's capture.
//!
//! A [`Scope`] is a plot set up the way an instrument is. Channels carry
//! their own volts per division and vertical offset, the timebase is seconds
//! per division rather than an x range, and a trigger picks where in the
//! captured samples the drawn window starts so a repeating signal stands still
//! instead of sliding across the screen.

use crate::render::{Color, Rect};

use super::axis::Axis;
use super::buffer::SampleBuffer;
use super::numeric::count_to_f64;
use super::project::Projection;
use super::scale::{Mapping, Range};
use super::ticks::{Tick, TickPlan};

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

/// An oscilloscope over one or more channels. Show it with
/// [`ScopeView`](crate::ui::ScopeView).
#[derive(Debug, Clone)]
pub struct Scope {
    pub channels: Vec<Channel>,
    pub timebase: Timebase,
    pub trigger: Trigger,
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
    /// Capacity handed to channels made by [`Scope::add_input`].
    default_capacity: usize,
    /// The graticule, in seconds and volts. Kept so its ticks are made
    /// again only when the timebase or a channel's scale changes.
    x: Axis,
    y: Axis,
    pub(crate) projection: Projection,
}

impl Scope {
    /// A scope whose channels each hold `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            channels: Vec::new(),
            timebase: Timebase::default(),
            trigger: Trigger::default(),
            vertical_divisions: 4,
            last_trigger: None,
            frozen: None,
            single_done: false,
            default_capacity: capacity,
            x: Axis::default().with_ticks(graticule("s")),
            y: Axis::default().with_ticks(graticule("V")),
            projection: Projection::default(),
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

    /// Settles the trigger for this frame: where the sweep starts, and the
    /// copy a single shot takes on the frame it fires.
    pub(crate) fn settle(&mut self) {
        let window = self.timebase.window_samples();
        // A stopped single shot keeps the sweep it has; looking for a newer
        // crossing is exactly what it is stopped from doing.
        if self.trigger.enabled && !self.is_stopped() {
            let source = self.channels.get(self.trigger.source).filter(|c| c.visible);
            if let Some(start) = source.and_then(|channel| self.find_trigger(channel, window)) {
                self.last_trigger = Some(start);
            }
        }
        if self.trigger.mode == TriggerMode::Single
            && self.last_trigger.is_some()
            && !self.single_done
        {
            // Take the copy now, on the frame it fired. Waiting would mean
            // capturing whatever the ring held by then instead.
            self.freeze_sweep(window);
        }
    }

    /// The samples channel `index` shows this frame, as at most two runs,
    /// without copying them.
    pub(crate) fn sweep(&self, index: usize) -> (&[f64], &[f64]) {
        if let Some(frozen) = &self.frozen {
            return (frozen.get(index).map_or(&[][..], Vec::as_slice), &[]);
        }
        let Some(channel) = self.channels.get(index) else {
            return (&[], &[]);
        };
        let (start, take) = self.window_for(channel, self.timebase.window_samples());
        channel.samples.range(start, take)
    }

    /// The graticule for this frame: even divisions in engineering units,
    /// the way an instrument front panel reads, mapped onto `area`.
    pub(crate) fn graticule(&mut self, area: Rect) -> (Mapping, Mapping) {
        let vertical = self.vertical_range();
        let seconds = self.timebase.window_seconds();
        // In place: a new plan each frame would mean a new suffix string.
        self.x.ticks.even_divisions = Some(self.timebase.divisions);
        self.y.ticks.even_divisions = Some(self.vertical_divisions * 2);
        self.x.resolve(None, Some(Range::new(0.0, seconds)));
        self.y.resolve(None, Some(vertical));
        let x_map = Mapping::new(self.x.range(), self.x.scale, area.x, area.right());
        let y_map = Mapping::new(self.y.range(), self.y.scale, area.bottom(), area.y);
        (x_map, y_map)
    }

    /// The graticule's ticks, from the last [`Self::graticule`].
    pub(crate) fn ticks(&self) -> (&[Tick], &[Tick]) {
        (self.x.resolved_ticks(), self.y.resolved_ticks())
    }
}

/// Ticks for a graticule in `unit`; the divisions are set each frame.
fn graticule(unit: &str) -> TickPlan {
    TickPlan::default().with_engineering(true).with_suffix(unit)
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
