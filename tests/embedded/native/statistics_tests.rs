use super::*;

mod oracle {
    include!("statistics_oracle_data.rs");
}

#[test]
fn stdtr_matches_scipy_oracle() {
    // Hypothesis's `test_stdtr_explicit` asserts agreement to rel=1e-12,
    // abs=1e-15 against scipy. Our port should match because the Python and
    // Rust implementations evaluate the identical Abramowitz & Stegun finite
    // sum in IEEE-754 doubles.
    for &(df, t, expected) in oracle::STDTR_CASES {
        let got = stdtr(df, t);
        assert_close(
            got,
            expected,
            1e-12,
            1e-15,
            &format!("stdtr(df={df}, t={t})"),
        );
    }
}

#[test]
fn stdtrit_strict_matches_scipy_oracle_for_df_le_2() {
    // df ∈ {1, 2} use the closed-form Shaw quantiles. Hypothesis asserts
    // rel=1e-15 against newer scipy; we use the same tolerance.
    for &(df, p, expected) in oracle::STDTRIT_STRICT_CASES {
        let got = stdtrit(df, p);
        assert_close(
            got,
            expected,
            1e-15,
            0.0,
            &format!("stdtrit(df={df}, p={p})"),
        );
    }
}

#[test]
fn stdtrit_lax_matches_scipy_oracle_for_df_ge_3() {
    // df >= 3 uses Newton-on-stdtr; Hypothesis asserts rel=1e-7, abs=1e-9.
    for &(df, p, expected) in oracle::STDTRIT_LAX_CASES {
        let got = stdtrit(df, p);
        assert_close(
            got,
            expected,
            1e-7,
            1e-9,
            &format!("stdtrit(df={df}, p={p})"),
        );
    }
}

#[test]
fn stdtr_is_symmetric_around_zero() {
    for df in [1, 2, 3, 4, 10, 50] {
        for t in [0.1, 0.5, 1.0, 2.0, 5.0, 100.0] {
            let lhs = stdtr(df, t);
            let rhs = 1.0 - stdtr(df, -t);
            assert_close(lhs, rhs, 1e-14, 1e-15, &format!("symmetry df={df}, t={t}"));
        }
    }
}

#[test]
fn stdtr_at_zero_is_one_half() {
    for df in [1, 2, 3, 4, 17, 100] {
        assert_eq!(stdtr(df, 0.0), 0.5);
    }
}

#[test]
fn stdtrit_at_one_half_is_zero() {
    for df in [1, 2, 3, 4, 17, 100] {
        assert_eq!(stdtrit(df, 0.5), 0.0);
    }
}

#[test]
fn stdtrit_inverts_stdtr() {
    // Strict on df∈{1, 2} (closed form); lax on df>=3 (Newton).
    for &(df, tol) in &[(1, 1e-12), (2, 1e-12), (3, 1e-7), (10, 1e-7)] {
        for t in [-5.0, -1.0, -0.1, 0.1, 1.0, 5.0] {
            let p = stdtr(df, t);
            let back = stdtrit(df, p);
            assert_close(back, t, tol, 1e-10, &format!("inverse df={df}, t={t}"));
        }
    }
}

#[test]
#[should_panic(expected = "df >= 1")]
fn stdtr_panics_on_df_zero() {
    stdtr(0, 0.0);
}

#[test]
#[should_panic(expected = "df >= 1")]
fn stdtrit_panics_on_df_zero() {
    stdtrit(0, 0.5);
}

#[test]
#[should_panic(expected = "0 < p < 1")]
fn stdtrit_panics_on_p_out_of_range_low() {
    stdtrit(2, 0.0);
}

#[test]
#[should_panic(expected = "0 < p < 1")]
fn stdtrit_panics_on_p_out_of_range_high() {
    stdtrit(2, 1.0);
}

#[test]
fn uniform_inverse_cdf_spans_range() {
    let d = UniformDistribution::new(256.0);
    assert_eq!(d.inverse_cdf(0.0), -256.0);
    assert_eq!(d.inverse_cdf(0.5), 0.0);
    assert_eq!(d.inverse_cdf(1.0), 256.0);
}

#[test]
fn uniform_cdf_linear() {
    let d = UniformDistribution::new(10.0);
    assert_eq!(d.cdf(-10.0), 0.0);
    assert_eq!(d.cdf(0.0), 0.5);
    assert_eq!(d.cdf(10.0), 1.0);
    assert_eq!(d.cdf(5.0), 0.75);
}

#[test]
fn uniform_pdf_is_reciprocal_of_total_width() {
    let d = UniformDistribution::new(10.0);
    assert_eq!(d.pdf(0.0), 1.0 / 20.0);
    assert_eq!(d.pdf(5.0), 1.0 / 20.0);
}

#[test]
fn uniform_cdf_inverse_cdf_round_trip() {
    let d = UniformDistribution::new(100.0);
    for u in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let x = d.inverse_cdf(u);
        let back = d.cdf(x);
        assert!((back - u).abs() < 1e-15);
    }
}

#[test]
fn log_student_t_cdf_at_zero() {
    let d = LogStudentTDistribution::new(13.0, 2);
    assert_eq!(d.cdf(0.0), 0.5);
}

#[test]
fn log_student_t_inverse_at_half_is_zero() {
    let d = LogStudentTDistribution::new(13.0, 2);
    assert_eq!(d.inverse_cdf(0.5), 0.0);
}

#[test]
fn log_student_t_round_trip() {
    let d = LogStudentTDistribution::new(13.0, 2);
    for x in [1.0, 10.0, 100.0, 1_000.0, 1_000_000.0] {
        let p = d.cdf(x);
        let back = d.inverse_cdf(p);
        // Small relative tolerance — Newton is lax but we're inside the
        // bulk where it converges well.
        let rel = (back - x).abs() / x.abs().max(1.0);
        assert!(rel < 1e-6, "round-trip failed for x={x}: back={back}");
    }
}

#[test]
fn log_student_t_clamps_extreme_inverse_cdf_inputs() {
    // The Python port clamps internal y to ±1023 before expm1 to avoid
    // OverflowError. Verify we return a finite float rather than panicking
    // or returning ±inf at the extreme tails.
    let d = LogStudentTDistribution::new(13.0, 2);
    let lo = d.inverse_cdf(1e-300);
    let hi = d.inverse_cdf(1.0 - 1e-15);
    assert!(lo.is_finite() && hi.is_finite(), "lo={lo}, hi={hi}");
    assert!(lo < 0.0 && hi > 0.0);
}

#[test]
fn log_student_t_pdf_is_positive_and_peaked_at_zero() {
    let d = LogStudentTDistribution::new(13.0, 2);
    let p0 = d.pdf(0.0);
    let p1 = d.pdf(1.0);
    let p1000 = d.pdf(1_000.0);
    assert!(p0 > 0.0);
    assert!(p0 > p1);
    assert!(p1 > p1000);
}

#[test]
fn piecewise_cdf_inverse_round_trip() {
    let inner = UniformDistribution::new(256.0);
    let outer = LogStudentTDistribution::new(13.0, 2);
    let d = PiecewiseDistribution::new(inner, outer, 256.0);
    for u in [0.01, 0.1, 0.3, 0.5, 0.7, 0.9, 0.99] {
        let x = d.inverse_cdf(u);
        let back = d.cdf(x);
        assert!(
            (back - u).abs() < 1e-9,
            "round-trip mismatch u={u}, x={x}, back={back}"
        );
    }
}

#[test]
fn piecewise_cdf_at_zero_is_one_half() {
    // Both branches symmetric around 0, so the splice should be too.
    let inner = UniformDistribution::new(256.0);
    let outer = LogStudentTDistribution::new(13.0, 2);
    let d = PiecewiseDistribution::new(inner, outer, 256.0);
    let c = d.cdf(0.0);
    assert!((c - 0.5).abs() < 1e-15, "cdf(0) = {c}");
}

#[test]
fn piecewise_inverse_cdf_at_half_is_zero() {
    let inner = UniformDistribution::new(256.0);
    let outer = LogStudentTDistribution::new(13.0, 2);
    let d = PiecewiseDistribution::new(inner, outer, 256.0);
    let x = d.inverse_cdf(0.5);
    assert!(x.abs() < 1e-12, "inverse_cdf(0.5) = {x}");
}

#[test]
fn piecewise_density_is_continuous_at_switchover() {
    // Density continuity is the whole point of the alpha/beta normalisation.
    let inner = UniformDistribution::new(256.0);
    let outer = LogStudentTDistribution::new(13.0, 2);
    let d = PiecewiseDistribution::new(inner, outer, 256.0);
    // Forward-difference approximations to the density from either side.
    let left_density = d.cdf(256.0) - d.cdf(255.0);
    let right_density = d.cdf(257.0) - d.cdf(256.0);
    let rel = (left_density - right_density).abs() / left_density;
    assert!(rel < 0.05, "left={left_density}, right={right_density}");
}

#[test]
fn piecewise_cdf_is_monotone_and_bounded() {
    let inner = UniformDistribution::new(256.0);
    let outer = LogStudentTDistribution::new(13.0, 2);
    let d = PiecewiseDistribution::new(inner, outer, 256.0);
    // CDF must lie in [0, 1] and be monotone non-decreasing across the
    // splice boundary. The log-student-t tails decay slowly so absolute
    // mass at ±1e3 is not yet near the endpoints, but monotonicity has to
    // hold everywhere.
    let xs = [
        -1e10, -1e6, -1000.0, -256.0, -100.0, -1.0, 0.0, 1.0, 100.0, 256.0, 1000.0, 1e6, 1e10,
    ];
    let mut prev = 0.0_f64;
    for &x in &xs {
        let c = d.cdf(x);
        assert!((0.0..=1.0).contains(&c), "cdf({x}) = {c} outside [0, 1]");
        assert!(c >= prev, "cdf not monotone at x={x}: {prev} -> {c}");
        prev = c;
    }
}

fn assert_close(got: f64, want: f64, rel: f64, abs: f64, ctx: &str) {
    let diff = (got - want).abs();
    let tol = abs.max(rel * want.abs().max(got.abs()));
    assert!(
        diff <= tol,
        "{ctx}: got {got}, want {want}, diff {diff}, tol {tol}"
    );
}
