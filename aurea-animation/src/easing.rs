/// Easing function selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EaseMode {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
}

impl EaseMode {
    /// Evaluate the easing function at `t ∈ [0, 1]`.
    /// Input is clamped; output is always in [0, 1].
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,

            Self::InQuad => t * t,
            Self::OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Self::InOutQuad => {
                if t < 0.5 { 2.0 * t * t }
                else { 1.0 - (-2.0 * t + 2.0) * (-2.0 * t + 2.0) / 2.0 }
            }

            Self::InCubic => t * t * t,
            Self::OutCubic => {
                let u = t - 1.0;
                1.0 + u * u * u
            }
            Self::InOutCubic => {
                if t < 0.5 { 4.0 * t * t * t }
                else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }

            Self::InQuart => t * t * t * t,
            Self::OutQuart => {
                let u = t - 1.0;
                1.0 - u * u * u * u
            }
            Self::InOutQuart => {
                if t < 0.5 { 8.0 * t * t * t * t }
                else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u * u / 2.0
                }
            }

            Self::InQuint => t * t * t * t * t,
            Self::OutQuint => {
                let u = t - 1.0;
                1.0 + u * u * u * u * u
            }
            Self::InOutQuint => {
                if t < 0.5 { 16.0 * t * t * t * t * t }
                else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u * u * u / 2.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool { (a - b).abs() < 1e-5 }

    #[test]
    fn linear_identity() {
        assert!(approx(EaseMode::Linear.eval(0.0), 0.0));
        assert!(approx(EaseMode::Linear.eval(0.5), 0.5));
        assert!(approx(EaseMode::Linear.eval(1.0), 1.0));
    }

    #[test]
    fn all_modes_boundary() {
        let modes = [
            EaseMode::InQuad, EaseMode::OutQuad, EaseMode::InOutQuad,
            EaseMode::InCubic, EaseMode::OutCubic, EaseMode::InOutCubic,
            EaseMode::InQuart, EaseMode::OutQuart, EaseMode::InOutQuart,
            EaseMode::InQuint, EaseMode::OutQuint, EaseMode::InOutQuint,
        ];
        for mode in modes {
            assert!(approx(mode.eval(0.0), 0.0), "{:?} must start at 0", mode);
            assert!(approx(mode.eval(1.0), 1.0), "{:?} must end at 1", mode);
        }
    }

    #[test]
    fn in_out_cubic_midpoint() {
        // InOutCubic must be exactly 0.5 at t=0.5 (symmetric by construction)
        assert!(approx(EaseMode::InOutCubic.eval(0.5), 0.5));
    }

    #[test]
    fn clamp_out_of_range() {
        assert!(approx(EaseMode::InQuad.eval(-1.0), 0.0));
        assert!(approx(EaseMode::OutQuad.eval(2.0), 1.0));
    }
}
