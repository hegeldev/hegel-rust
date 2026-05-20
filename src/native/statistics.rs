// Vendored port of Hypothesis's `internal/statistics.py` (PR
// HypothesisWorks/hypothesis#4728). Provides the numerical building blocks
// used by the integer sampler:
//
//   * `stdtr` / `stdtrit` — Student's t CDF and quantile for integer df.
//   * `UniformDistribution`, `LogStudentTDistribution` — primitive
//     distributions over the reals.
//   * `PiecewiseDistribution` — two-region splice used to combine a uniform
//     inner core with heavy-tailed outer behaviour.
//
// Tests live in `tests/embedded/native/statistics_tests.rs` and are
// validated against a scipy-derived oracle (see
// `scripts/generate_statistics_oracle.py`). The `stdtr` / `stdtrit`
// implementations were authored against scipy as oracle on the Python side
// and re-validated against the same oracle here.

use std::f64::consts::PI;

/// Student's t CDF for integer `df >= 1`, evaluated at `t`.
///
/// Closed-form finite sum from Abramowitz & Stegun 26.7.7-8. Port of
/// `hypothesis.internal.statistics.stdtr`.
pub fn stdtr(df: i32, t: f64) -> f64 {
    assert!(df >= 1, "stdtr requires integer df >= 1, got {df}");
    if t == 0.0 {
        return 0.5;
    }
    let abs_t = t.abs();
    let z = 1.0 + abs_t * abs_t / df as f64;
    let p = if df % 2 == 1 {
        // odd df: includes an arctan term
        let u = abs_t / (df as f64).sqrt();
        let mut p = u.atan();
        if df > 1 {
            let mut f = 1.0_f64;
            let mut tz = 1.0_f64;
            let mut j = 3i32;
            while j <= df - 2 {
                tz *= (j as f64 - 1.0) / (z * j as f64);
                f += tz;
                j += 2;
            }
            p += f * u / z;
        }
        p * 2.0 / PI
    } else {
        // even df: simple finite sum, no arctan
        let mut f = 1.0_f64;
        let mut tz = 1.0_f64;
        let mut j = 2i32;
        while j <= df - 2 {
            tz *= (j as f64 - 1.0) / (z * j as f64);
            f += tz;
            j += 2;
        }
        f * abs_t / (z * df as f64).sqrt()
    };
    if t < 0.0 {
        0.5 - 0.5 * p
    } else {
        0.5 + 0.5 * p
    }
}

/// Inverse Student's t CDF (quantile) for integer `df >= 1`.
///
/// * `df ∈ {1, 2}`: closed-form analytic quantile (Shaw 2006, eq 35-36).
/// * `df >= 3`: bracketed Newton iteration on [`stdtr`].
///
/// Port of `hypothesis.internal.statistics.stdtrit`.
pub fn stdtrit(df: i32, p: f64) -> f64 {
    assert!(df >= 1, "stdtrit requires integer df >= 1, got {df}");
    if p == 0.5 {
        return 0.0;
    }
    assert!(p > 0.0 && p < 1.0, "stdtrit requires 0 < p < 1, got {p}");
    if df == 1 {
        // Cauchy: F^{-1}(p) = -cot(pi p). Reflect via 1-p when p > 0.5
        // because sin(pi p) near pi suffers cancellation; sin(pi (1-p))
        // near 0 is exact.
        return if p > 0.5 {
            (PI * (1.0 - p)).cos() / (PI * (1.0 - p)).sin()
        } else {
            -(PI * p).cos() / (PI * p).sin()
        };
    }
    if df == 2 {
        return (2.0 * p - 1.0) / (2.0 * p * (1.0 - p)).sqrt();
    }

    let sign = if p > 0.5 { 1.0 } else { -1.0 };
    let q = if p > 0.5 { p } else { 1.0 - p };

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    while stdtr(df, hi) < q {
        hi *= 2.0;
    }

    let log_norm =
        ln_gamma(0.5 * (df as f64 + 1.0)) - 0.5 * (df as f64 * PI).ln() - ln_gamma(0.5 * df as f64);
    let mut t = 0.5 * (lo + hi);
    let eps = 1e-10;
    for _ in 0..50 {
        let f_val = stdtr(df, t);
        if f_val < q {
            lo = t;
        } else {
            hi = t;
        }
        let log_f = log_norm - 0.5 * (df as f64 + 1.0) * (t * t / df as f64).ln_1p();
        let f = log_f.exp();
        // `f == 0.0` would yield NaN/±inf for `t_newton`, but the
        // bracket check rejects those (NaN/inf comparisons are false)
        // and falls through to the bisection step — so an explicit
        // underflow guard would be redundant.
        let t_newton = t - (f_val - q) / f;
        t = if lo <= t_newton && t_newton <= hi {
            t_newton
        } else {
            0.5 * (lo + hi)
        };
        if hi - lo < eps * (1.0 + t.abs()) {
            break;
        }
    }
    sign * t
}

/// `ln(Γ(x))` for `x >= 0.5`. Rust stdlib doesn't expose this and the
/// callers (Newton iteration for `stdtrit` with `df >= 3`, and the
/// log-Student-t pdf normaliser) only ever pass `x >= 1.5`, so the
/// reflection formula needed for `x < 0.5` is not implemented.
///
/// Lanczos approximation (g=7, 9 coefficients) — accurate to ~1e-15 for
/// the inputs we actually use (`x = (df+1)/2` and `x = df/2` with
/// `df >= 3`).
fn ln_gamma(x: f64) -> f64 {
    // Lanczos g=7, coefficients from Numerical Recipes / Wikipedia.
    const COEFFS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let z = x - 1.0;
    let mut sum = COEFFS[0];
    for (i, &c) in COEFFS.iter().enumerate().skip(1) {
        sum += c / (z + i as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * PI).ln() + (z + 0.5) * t.ln() - t + sum.ln()
}

/// `Γ(x)`. Same Lanczos series as [`ln_gamma`], exponentiated. Kept
/// separate so the [`LogStudentTDistribution`] coefficient cache reads
/// naturally as `Γ((df+1)/2) / (√(df π) · Γ(df/2))`.
fn gamma(x: f64) -> f64 {
    ln_gamma(x).exp()
}

/// Common interface for the primitive distributions composed by
/// [`PiecewiseDistribution`].
///
/// All concrete implementations in this module are symmetric around zero,
/// so `inverse_cdf(0.5) == 0`. The piecewise splice relies on that.
pub trait Distribution {
    fn cdf(&self, x: f64) -> f64;
    fn inverse_cdf(&self, u: f64) -> f64;
    fn pdf(&self, x: f64) -> f64;
}

/// Uniform distribution on `[-half_width, half_width]`.
pub struct UniformDistribution {
    half_width: f64,
}

impl UniformDistribution {
    pub fn new(half_width: f64) -> Self {
        Self { half_width }
    }
}

impl Distribution for UniformDistribution {
    fn cdf(&self, x: f64) -> f64 {
        if x < -self.half_width {
            return 0.0;
        }
        if x > self.half_width {
            return 1.0;
        }
        (x + self.half_width) / (2.0 * self.half_width)
    }

    fn inverse_cdf(&self, u: f64) -> f64 {
        -self.half_width + 2.0 * self.half_width * u
    }

    fn pdf(&self, x: f64) -> f64 {
        if -self.half_width <= x && x <= self.half_width {
            1.0 / (2.0 * self.half_width)
        } else {
            0.0
        }
    }
}

/// Student's t distribution in the transformed domain `Y = sign(x) ·
/// log_2(1 + |x|) ~ scale_bits · t(df)`.
///
/// Heavy-tailed by construction: tail decay is `1/|x|^(df+1)`. Used as the
/// outer arm of [`PiecewiseDistribution`] in the integer sampler so that
/// the magnitude grows on a log scale across many decades.
pub struct LogStudentTDistribution {
    scale_bits: f64,
    df: i32,
    t_coef: f64,
}

impl LogStudentTDistribution {
    pub fn new(scale_bits: f64, df: i32) -> Self {
        // Coefficient of the standard Student's t pdf at y=0, cached so
        // `pdf` is cheap.
        let t_coef =
            gamma((df as f64 + 1.0) / 2.0) / ((df as f64 * PI).sqrt() * gamma(df as f64 / 2.0));
        Self {
            scale_bits,
            df,
            t_coef,
        }
    }
}

const LN_2: f64 = std::f64::consts::LN_2;

impl Distribution for LogStudentTDistribution {
    fn cdf(&self, x: f64) -> f64 {
        let y = (1.0 + x.abs()).log2().copysign(x) / self.scale_bits;
        stdtr(self.df, y)
    }

    fn inverse_cdf(&self, u: f64) -> f64 {
        let mut y = self.scale_bits * stdtrit(self.df, u);
        // 2^1023 is the largest power of 2 below f64::MAX. Clamp so the
        // subsequent expm1 doesn't return ±inf at the extreme tails.
        y = y.clamp(-1023.0, 1023.0);
        (y.abs() * LN_2).exp_m1().copysign(y)
    }

    fn pdf(&self, x: f64) -> f64 {
        let y = (1.0 + x.abs()).log2().copysign(x) / self.scale_bits;
        let f_t =
            self.t_coef * (1.0 + y * y / self.df as f64).powf(-((self.df as f64 + 1.0) / 2.0));
        f_t / (self.scale_bits * (1.0 + x.abs()) * LN_2)
    }
}

/// Two-region splice: `inner` on `(-switchover, switchover)`, `outer` on
/// `|x| >= switchover`.
///
/// Each region is renormalised so the resulting density is continuous at
/// `±switchover` and integrates to 1. Both inner and outer must be
/// symmetric around 0.
pub struct PiecewiseDistribution<I: Distribution, O: Distribution> {
    inner: I,
    outer: O,
    switchover: f64,
    alpha: f64,
    beta: f64,
    inner_g_neg: f64,
    outer_g_pos: f64,
    inner_mass: f64,
    left_mass: f64,
}

impl<I: Distribution, O: Distribution> PiecewiseDistribution<I, O> {
    pub fn new(inner: I, outer: O, switchover: f64) -> Self {
        let inner_g_neg = inner.cdf(-switchover);
        let inner_g_pos = inner.cdf(switchover);
        let outer_g_neg = outer.cdf(-switchover);
        let outer_g_pos = outer.cdf(switchover);
        let outer_outer_mass = 1.0 - (outer_g_pos - outer_g_neg);
        let inner_inner_mass = inner_g_pos - inner_g_neg;
        let inner_pdf = inner.pdf(switchover);
        let outer_pdf = outer.pdf(switchover);
        assert!(
            inner_pdf != 0.0,
            "inner.pdf(switchover={switchover}) == 0; cannot density-match"
        );
        // Density continuity: beta * inner.pdf(c) = alpha * outer.pdf(c)
        // plus total mass 1; solve for (alpha, beta).
        let alpha = 1.0 / (outer_pdf * inner_inner_mass / inner_pdf + outer_outer_mass);
        let beta = alpha * outer_pdf / inner_pdf;
        let inner_mass = beta * inner_inner_mass;
        let left_mass = alpha * outer_g_neg;
        Self {
            inner,
            outer,
            switchover,
            alpha,
            beta,
            inner_g_neg,
            outer_g_pos,
            inner_mass,
            left_mass,
        }
    }
}

impl<I: Distribution, O: Distribution> Distribution for PiecewiseDistribution<I, O> {
    fn cdf(&self, x: f64) -> f64 {
        if x <= -self.switchover {
            self.alpha * self.outer.cdf(x)
        } else if x < self.switchover {
            self.left_mass + self.beta * (self.inner.cdf(x) - self.inner_g_neg)
        } else {
            self.left_mass + self.inner_mass + self.alpha * (self.outer.cdf(x) - self.outer_g_pos)
        }
    }

    fn inverse_cdf(&self, u: f64) -> f64 {
        if u <= self.left_mass {
            self.outer.inverse_cdf(u / self.alpha)
        } else if u < self.left_mass + self.inner_mass {
            let target = self.inner_g_neg + (u - self.left_mass) / self.beta;
            self.inner.inverse_cdf(target)
        } else {
            self.outer
                .inverse_cdf((u - self.left_mass - self.inner_mass) / self.alpha + self.outer_g_pos)
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x.abs() < self.switchover {
            self.beta * self.inner.pdf(x)
        } else {
            self.alpha * self.outer.pdf(x)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/statistics_tests.rs"]
mod tests;
