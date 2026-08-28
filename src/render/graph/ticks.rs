//! Choosing where the gridlines and labels go.
//!
//! Ticks land on round numbers, not on evenly divided range ends, because a
//! grid at 0.333 and 0.667 is unreadable while one at 0.25 and 0.5 is not. The
//! step is always 1, 2 or 5 times a power of ten.

use super::scale::{Range, Scale};

/// One gridline, with the text that goes beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// Where the tick sits, in data values.
    pub value: f64,
    /// What to write next to it. Empty for a minor tick.
    pub label: String,
    /// Whether this one gets a label and a heavier line.
    pub major: bool,
}

/// How ticks are chosen for an axis.
#[derive(Debug, Clone)]
pub struct TickPlan {
    /// Roughly how many major ticks to aim for. The real count lands near it,
    /// because the step is rounded to something readable.
    pub target: usize,
    /// Minor ticks between each pair of major ones. Zero for none.
    pub minor_per_major: usize,
    /// Fixed decimal places for labels. `None` picks a count from the step so
    /// neighbouring labels stay distinct.
    pub decimals: Option<usize>,
    /// Appended to every label, e.g. a unit.
    pub suffix: String,
}

impl Default for TickPlan {
    fn default() -> Self {
        Self {
            target: 6,
            minor_per_major: 0,
            decimals: None,
            suffix: String::new(),
        }
    }
}

impl TickPlan {
    /// Aims for `target` major ticks.
    pub fn with_target(mut self, target: usize) -> Self {
        self.target = target;
        self
    }

    /// Puts `count` minor ticks between major ones.
    pub fn with_minor(mut self, count: usize) -> Self {
        self.minor_per_major = count;
        self
    }

    /// Fixes how many decimals every label carries.
    pub fn with_decimals(mut self, decimals: usize) -> Self {
        self.decimals = Some(decimals);
        self
    }

    /// Appends `suffix` to every label.
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Works out the ticks for `range` under `scale`.
    ///
    /// Comes back empty when the range has no width or the numbers are not
    /// finite, which is the honest answer: there is nowhere to put a gridline.
    pub fn ticks(&self, range: Range, scale: Scale) -> Vec<Tick> {
        match scale {
            Scale::Linear => self.linear_ticks(range),
            Scale::Log10 => self.log_ticks(range),
        }
    }

    fn linear_ticks(&self, range: Range) -> Vec<Tick> {
        if range.is_degenerate() || self.target == 0 {
            return Vec::new();
        }
        let (lo, hi) = ordered(range);
        let step = nice_step((hi - lo) / super::numeric::count_to_f64(self.target));
        if step <= 0.0 || !step.is_finite() {
            return Vec::new();
        }

        let decimals = self.decimals.unwrap_or_else(|| decimals_for(step));
        let minor_step = self.minor_step(step);

        let mut ticks = Vec::new();
        let first = (lo / step).ceil();
        // Guard against a step so small the loop would never end.
        let count = ((hi - lo) / step).ceil();
        if !count.is_finite() || count > MAX_TICKS {
            return Vec::new();
        }

        let mut i = 0.0;
        loop {
            let value = (first + i) * step;
            if value > hi + step * 1e-9 {
                break;
            }
            if value >= lo - step * 1e-9 {
                ticks.push(Tick {
                    value,
                    label: self.format(value, decimals),
                    major: true,
                });
                self.push_minor(&mut ticks, value, minor_step, lo, hi);
            }
            i += 1.0;
        }
        ticks
    }

    /// Minor ticks sit between this major tick and the next one.
    fn push_minor(&self, ticks: &mut Vec<Tick>, major: f64, minor_step: f64, lo: f64, hi: f64) {
        if minor_step <= 0.0 {
            return;
        }
        for m in 1..=self.minor_per_major {
            let value = minor_step.mul_add(super::numeric::count_to_f64(m), major);
            if value > hi || value < lo {
                continue;
            }
            ticks.push(Tick {
                value,
                label: String::new(),
                major: false,
            });
        }
    }

    fn minor_step(&self, step: f64) -> f64 {
        if self.minor_per_major == 0 {
            return 0.0;
        }
        step / super::numeric::count_to_f64(self.minor_per_major + 1)
    }

    fn log_ticks(&self, range: Range) -> Vec<Tick> {
        if range.min <= 0.0 || range.max <= 0.0 || range.is_degenerate() {
            return Vec::new();
        }
        let (lo, hi) = ordered(range);
        let first = lo.log10().floor();
        let last = hi.log10().ceil();
        if !first.is_finite() || !last.is_finite() || last - first > MAX_TICKS {
            return Vec::new();
        }

        let decimals = self.decimals;
        let mut ticks = Vec::new();
        let mut decade = first;
        while decade <= last {
            let base = 10f64.powf(decade);
            if base >= lo && base <= hi {
                ticks.push(Tick {
                    value: base,
                    label: self.format(base, decimals.unwrap_or_else(|| decimals_for(base))),
                    major: true,
                });
            }
            // 2x through 9x inside the decade, unlabelled.
            if self.minor_per_major > 0 {
                for m in 2..=9 {
                    let value = base * f64::from(m);
                    if value >= lo && value <= hi {
                        ticks.push(Tick {
                            value,
                            label: String::new(),
                            major: false,
                        });
                    }
                }
            }
            decade += 1.0;
        }
        ticks
    }

    fn format(&self, value: f64, decimals: usize) -> String {
        // -0 reads as a mistake on an axis, so fold it into 0.
        let value = if value == 0.0 { 0.0 } else { value };
        format!("{value:.decimals$}{}", self.suffix)
    }
}

/// Anything past this and the axis is not readable anyway; better to draw no
/// grid than to spend the frame emitting gridlines.
const MAX_TICKS: f64 = 10_000.0;

fn ordered(range: Range) -> (f64, f64) {
    if range.min <= range.max {
        (range.min, range.max)
    } else {
        (range.max, range.min)
    }
}

/// Rounds a raw step up to 1, 2 or 5 times a power of ten.
fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 || !raw.is_finite() {
        return 0.0;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalised = raw / magnitude;
    let stepped = if normalised <= 1.0 {
        1.0
    } else if normalised <= 2.0 {
        2.0
    } else if normalised <= 5.0 {
        5.0
    } else {
        10.0
    };
    stepped * magnitude
}

/// Enough decimals that neighbouring labels differ.
fn decimals_for(step: f64) -> usize {
    if step <= 0.0 || !step.is_finite() {
        return 0;
    }
    let exponent = step.log10().floor();
    if exponent >= 0.0 {
        0
    } else {
        // -1 needs one decimal, -2 needs two, and so on. Capped so a tiny step
        // does not produce an unreadable label.
        super::numeric::f64_to_count((-exponent).clamp(0.0, 9.0).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(ticks: &[Tick]) -> Vec<f64> {
        ticks.iter().filter(|t| t.major).map(|t| t.value).collect()
    }

    #[test]
    fn steps_round_to_one_two_or_five() {
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(1.5), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(7.0), 10.0);
        assert!((nice_step(0.03) - 0.05).abs() < 1e-12);
        assert!((nice_step(230.0) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn a_zero_or_bad_step_is_rejected() {
        assert_eq!(nice_step(0.0), 0.0);
        assert_eq!(nice_step(-1.0), 0.0);
        assert_eq!(nice_step(f64::NAN), 0.0);
    }

    #[test]
    fn ticks_land_on_round_numbers() {
        let plan = TickPlan::default().with_target(5);
        let ticks = plan.ticks(Range::new(0.0, 10.0), Scale::Linear);

        assert_eq!(values(&ticks), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(ticks[0].label, "0");
    }

    #[test]
    fn ticks_stay_inside_an_awkward_range() {
        let plan = TickPlan::default().with_target(4);
        let ticks = plan.ticks(Range::new(-3.3, 7.1), Scale::Linear);

        for tick in &ticks {
            assert!(
                tick.value >= -3.3 - 1e-9 && tick.value <= 7.1 + 1e-9,
                "{} escaped the range",
                tick.value
            );
        }
        assert!(!ticks.is_empty());
    }

    #[test]
    fn a_flat_range_has_no_ticks() {
        let plan = TickPlan::default();
        assert!(plan.ticks(Range::new(5.0, 5.0), Scale::Linear).is_empty());
        assert!(
            plan.ticks(Range::new(f64::NAN, 1.0), Scale::Linear)
                .is_empty()
        );
    }

    #[test]
    fn a_target_of_zero_has_no_ticks() {
        let plan = TickPlan::default().with_target(0);
        assert!(plan.ticks(Range::new(0.0, 10.0), Scale::Linear).is_empty());
    }

    #[test]
    fn minor_ticks_sit_between_the_major_ones_and_carry_no_label() {
        let plan = TickPlan::default().with_target(5).with_minor(1);
        let ticks = plan.ticks(Range::new(0.0, 10.0), Scale::Linear);

        let minors: Vec<f64> = ticks.iter().filter(|t| !t.major).map(|t| t.value).collect();
        assert!(minors.contains(&1.0), "got {minors:?}");
        assert!(
            ticks
                .iter()
                .filter(|t| !t.major)
                .all(|t| t.label.is_empty())
        );
    }

    #[test]
    fn labels_get_enough_decimals_to_stay_apart() {
        let plan = TickPlan::default().with_target(5);
        let ticks = plan.ticks(Range::new(0.0, 1.0), Scale::Linear);

        let labels: Vec<&str> = ticks.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"0.2"), "got {labels:?}");
    }

    #[test]
    fn decimals_can_be_fixed() {
        let plan = TickPlan::default().with_target(5).with_decimals(3);
        let ticks = plan.ticks(Range::new(0.0, 10.0), Scale::Linear);
        assert_eq!(ticks[0].label, "0.000");
    }

    #[test]
    fn a_suffix_lands_on_every_label() {
        let plan = TickPlan::default().with_target(2).with_suffix(" V");
        let ticks = plan.ticks(Range::new(0.0, 10.0), Scale::Linear);
        assert!(
            ticks
                .iter()
                .filter(|t| t.major)
                .all(|t| t.label.ends_with(" V"))
        );
    }

    #[test]
    fn negative_zero_is_written_as_zero() {
        let plan = TickPlan::default().with_target(4);
        let ticks = plan.ticks(Range::new(-1.0, 1.0), Scale::Linear);
        assert!(!ticks.iter().any(|t| t.label.starts_with("-0.0")));
        assert!(!ticks.iter().any(|t| t.label == "-0"));
    }

    #[test]
    fn log_ticks_sit_on_the_decades() {
        let plan = TickPlan::default();
        let ticks = plan.ticks(Range::new(1.0, 1000.0), Scale::Log10);
        assert_eq!(values(&ticks), vec![1.0, 10.0, 100.0, 1000.0]);
    }

    #[test]
    fn log_minor_ticks_fill_the_decade() {
        let plan = TickPlan::default().with_minor(8);
        let ticks = plan.ticks(Range::new(1.0, 10.0), Scale::Log10);

        let minors: Vec<f64> = ticks.iter().filter(|t| !t.major).map(|t| t.value).collect();
        assert!(
            minors.contains(&2.0) && minors.contains(&9.0),
            "got {minors:?}"
        );
    }

    #[test]
    fn a_log_axis_through_zero_has_no_ticks() {
        let plan = TickPlan::default();
        assert!(plan.ticks(Range::new(0.0, 100.0), Scale::Log10).is_empty());
        assert!(plan.ticks(Range::new(-5.0, 100.0), Scale::Log10).is_empty());
    }

    /// A tiny step over a wide range would otherwise spin emitting gridlines
    /// no one can see.
    #[test]
    fn an_absurd_tick_count_is_refused() {
        let plan = TickPlan::default().with_target(100_000_000);
        assert!(plan.ticks(Range::new(0.0, 1e9), Scale::Linear).is_empty());
    }
}
