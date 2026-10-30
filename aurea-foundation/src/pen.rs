//! App-owned pen sampling and stabilization. Native readings stay unchanged.

use std::time::Duration;

use crate::{PenPhase, PenSample};

/// Knobs for processing actual pen readings in a brush or tool.
///
/// Defaults deliver every reading unchanged. These settings control app-side
/// delivery and smoothing, not the tablet's hardware sampling frequency.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PenSampling {
    /// Minimum elapsed time between emitted move readings. Zero keeps every reading.
    pub min_interval: Duration,
    /// Minimum travel from the last emitted position, in logical pixels.
    /// Both this threshold and `min_interval` must pass. Zero disables this gate.
    /// Non-finite or negative values are treated as zero.
    pub min_distance: f64,
    /// Position filter time constant. Zero disables stabilization; larger values
    /// suppress more jitter and add more lag. A starting point is 10–30 ms.
    pub position_smoothing: Duration,
    /// Separate pressure filter time constant. Zero preserves reported pressure.
    pub pressure_smoothing: Duration,
    /// Reset stabilization after this much time without a reading. Zero disables
    /// gap detection. Down, up, hover, tool changes, and clock resets always reset.
    pub reset_after: Duration,
}

impl PenSampling {
    /// Caps emitted moves at `hz` readings/second. Zero disables the rate gate.
    /// The source must provide at least this many readings to reach the cap.
    pub fn at_hz(hz: u32) -> Self {
        Self {
            min_interval: if hz == 0 {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(1.0 / f64::from(hz.min(1_000_000_000)))
            },
            ..Self::default()
        }
    }
}

/// A constant-memory pen sampler for one active pen stream.
///
/// Feed actual samples in timestamp order, including samples inside `PenBatch`.
/// Filtering runs on every reading, before delivery gates, so batch boundaries
/// do not change the result. Position and pressure use a first-order low-pass
/// filter with coefficient `dt / (time_constant + dt)`.
///
/// Down, up, and hover bypass the gates and are forwarded unchanged. Up reaches
/// the reported endpoint immediately, without a delayed tail. Tool and button
/// changes also bypass delivery gates. Predictions should use a separate sampler
/// or the app's preview path; they must not advance the actual stroke's history.
/// Call [`Self::reset`] on cancellation, focus loss, or when switching documents.
/// Non-finite positions or pressures are rejected and reset history.
///
/// ```
/// use aurea_foundation::{PenSampler, PenSampling, WindowEvent};
/// use std::time::Duration;
/// let mut sampler = PenSampler::new(PenSampling {
///     position_smoothing: Duration::from_millis(20),
///     pressure_smoothing: Duration::from_millis(8),
///     reset_after: Duration::from_millis(250),
///     ..PenSampling::at_hz(240)
/// });
/// # let event = WindowEvent::CloseRequested;
/// for raw in event.pen_samples() {
///     if let Some(stabilized) = sampler.sample(*raw) {
///         // Feed stabilized to the brush. `raw` is still available for recording.
///         # let _ = stabilized;
///     }
/// }
/// ```
#[derive(Debug, Default)]
pub struct PenSampler {
    options: PenSampling,
    previous: Option<PenSample>,
    emitted: Option<PenSample>,
}

impl PenSampler {
    /// Creates a sampler with no stroke history.
    pub fn new(options: PenSampling) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    /// Current sampling and stabilization settings.
    pub fn options(&self) -> PenSampling {
        self.options
    }

    /// Changes settings and resets history. The next valid reading is emitted.
    pub fn set_options(&mut self, options: PenSampling) {
        self.options = options;
        self.reset();
    }

    /// Forgets the current stroke without allocating or emitting a sample.
    pub fn reset(&mut self) {
        self.previous = None;
        self.emitted = None;
    }

    /// Processes one actual reading. `None` means a move was gated or invalid.
    /// All metadata except filtered move position and pressure is preserved.
    pub fn sample(&mut self, raw: PenSample) -> Option<PenSample> {
        if !raw.x.is_finite() || !raw.y.is_finite() || !raw.pressure.is_finite() {
            self.reset();
            return None;
        }
        if raw.phase != PenPhase::Move {
            self.reset();
            if raw.phase == PenPhase::Down {
                self.keep(raw);
            }
            return Some(raw);
        }
        let Some(previous) = self.previous else {
            return Some(self.keep(raw));
        };
        let elapsed = raw.time.saturating_sub(previous.time);
        if raw.tool != previous.tool
            || raw.time <= previous.time
            || (!self.options.reset_after.is_zero() && elapsed > self.options.reset_after)
        {
            return Some(self.keep(raw));
        }
        let mut filtered = raw;
        if !self.options.position_smoothing.is_zero() {
            let dt = elapsed.as_secs_f64();
            let a = dt / (self.options.position_smoothing.as_secs_f64() + dt);
            filtered.x = previous.x * (1.0 - a) + raw.x * a;
            filtered.y = previous.y * (1.0 - a) + raw.y * a;
        }
        if !self.options.pressure_smoothing.is_zero() {
            let dt = elapsed.as_secs_f32();
            let a = dt / (self.options.pressure_smoothing.as_secs_f32() + dt);
            filtered.pressure = previous.pressure * (1.0 - a) + raw.pressure * a;
        }
        self.previous = Some(filtered);
        if let Some(emitted) = self.emitted {
            let controls_changed =
                raw.barrel != emitted.barrel || raw.modifiers != emitted.modifiers;
            let distance = if self.options.min_distance.is_finite() {
                self.options.min_distance.max(0.0)
            } else {
                0.0
            };
            if !controls_changed
                && (raw.time.saturating_sub(emitted.time) < self.options.min_interval
                    || (distance > 0.0
                        && (filtered.x - emitted.x).hypot(filtered.y - emitted.y) < distance))
            {
                return None;
            }
        }
        self.emitted = Some(filtered);
        Some(filtered)
    }

    fn keep(&mut self, sample: PenSample) -> PenSample {
        self.previous = Some(sample);
        self.emitted = Some(sample);
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PenTool, WindowEvent};

    fn reading(ms: u64, x: f64, phase: PenPhase) -> PenSample {
        PenSample {
            x,
            y: x * 0.5,
            pressure: 0.5,
            time: Duration::from_millis(ms),
            phase,
            ..PenSample::default()
        }
    }

    #[test]
    fn defaults_preserve_every_reading_and_metadata() {
        let mut sampler = PenSampler::default();
        for phase in [
            PenPhase::Hover,
            PenPhase::Down,
            PenPhase::Move,
            PenPhase::Up,
        ] {
            let raw = PenSample {
                tilt_x: 21.0,
                tilt_y: -10.0,
                twist: 350.0,
                barrel: true,
                tool: PenTool::Eraser,
                ..reading(10, 2.25, phase)
            };
            assert_eq!(sampler.sample(raw), Some(raw));
        }
        assert_eq!(PenSampling::at_hz(0), PenSampling::default());
        assert_eq!(
            PenSampling::at_hz(100).min_interval,
            Duration::from_millis(10)
        );
    }

    #[test]
    fn rate_and_distance_gate_moves_but_never_stroke_boundaries() {
        let mut sampler = PenSampler::new(PenSampling {
            min_distance: 2.0,
            ..PenSampling::at_hz(100)
        });
        assert!(sampler.sample(reading(0, 0.0, PenPhase::Down)).is_some());
        assert!(sampler.sample(reading(5, 4.0, PenPhase::Move)).is_none());
        assert!(sampler.sample(reading(10, 0.5, PenPhase::Move)).is_none());
        assert!(sampler.sample(reading(11, 4.0, PenPhase::Move)).is_some());
        let changed = PenSample {
            barrel: true,
            ..reading(12, 4.1, PenPhase::Move)
        };
        assert_eq!(sampler.sample(changed), Some(changed));
        let up = reading(13, 4.2, PenPhase::Up);
        assert_eq!(sampler.sample(up), Some(up));
        let down = reading(14, 8.0, PenPhase::Down);
        assert_eq!(sampler.sample(down), Some(down));
    }

    #[test]
    fn smoothing_is_time_based_and_independent_for_pressure() {
        let mut sampler = PenSampler::new(PenSampling {
            position_smoothing: Duration::from_millis(30),
            ..Default::default()
        });
        sampler.sample(reading(0, 0.0, PenPhase::Down));
        let raw = PenSample {
            pressure: 1.0,
            ..reading(10, 10.0, PenPhase::Move)
        };
        let filtered = sampler.sample(raw).unwrap();
        assert_eq!(
            (filtered.x, filtered.y, filtered.pressure),
            (2.5, 1.25, 1.0)
        );
        assert_eq!((filtered.time, filtered.phase), (raw.time, raw.phase));
        sampler.set_options(PenSampling {
            pressure_smoothing: Duration::from_millis(30),
            ..Default::default()
        });
        sampler.sample(reading(0, 0.0, PenPhase::Down));
        let filtered = sampler.sample(raw).unwrap();
        assert_eq!((filtered.x, filtered.pressure), (10.0, 0.625));
        let up = reading(11, 12.0, PenPhase::Up);
        assert_eq!(sampler.sample(up), Some(up), "no delayed tail after pen up");
    }

    #[test]
    fn gated_readings_still_advance_stabilization() {
        let mut sampler = PenSampler::new(PenSampling {
            position_smoothing: Duration::from_millis(4),
            ..PenSampling::at_hz(125)
        });
        sampler.sample(reading(0, 0.0, PenPhase::Down));
        assert!(sampler.sample(reading(4, 8.0, PenPhase::Move)).is_none());
        assert_eq!(
            sampler.sample(reading(8, 8.0, PenPhase::Move)).unwrap().x,
            6.0
        );
    }

    #[test]
    fn batches_and_single_readings_have_identical_history() {
        let options = PenSampling {
            position_smoothing: Duration::from_millis(20),
            pressure_smoothing: Duration::from_millis(10),
            ..PenSampling::at_hz(100)
        };
        let readings: Vec<_> = (0..100u32)
            .map(|i| {
                reading(
                    u64::from(i),
                    f64::from(i),
                    if i == 0 {
                        PenPhase::Down
                    } else {
                        PenPhase::Move
                    },
                )
            })
            .collect();
        let mut a = PenSampler::new(options);
        let want: Vec<_> = readings.iter().filter_map(|s| a.sample(*s)).collect();
        let mut b = PenSampler::new(options);
        let events = [
            WindowEvent::Pen(readings[0]),
            WindowEvent::PenBatch(readings[1..40].to_vec()),
            WindowEvent::PenBatch(readings[40..].to_vec()),
        ];
        let got: Vec<_> = events
            .iter()
            .flat_map(WindowEvent::pen_samples)
            .filter_map(|s| b.sample(*s))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn discontinuities_and_invalid_input_do_not_leak_history() {
        let mut sampler = PenSampler::new(PenSampling {
            position_smoothing: Duration::from_secs(1),
            reset_after: Duration::from_millis(50),
            ..PenSampling::at_hz(1)
        });
        sampler.sample(reading(0, 0.0, PenPhase::Down));
        let after_gap = reading(100, 100.0, PenPhase::Move);
        assert_eq!(sampler.sample(after_gap), Some(after_gap));
        let tool = PenSample {
            tool: PenTool::Eraser,
            ..reading(101, 200.0, PenPhase::Move)
        };
        assert_eq!(sampler.sample(tool), Some(tool));
        let reversed = reading(1, 300.0, PenPhase::Move);
        assert_eq!(sampler.sample(reversed), Some(reversed));
        let invalid = PenSample {
            x: f64::NAN,
            ..reversed
        };
        assert!(sampler.sample(invalid).is_none());
        assert_eq!(sampler.sample(reversed), Some(reversed));
        sampler.reset();
        assert_eq!(sampler.sample(reversed), Some(reversed));
    }
}
