// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ATANH` in double-double arithmetic, rounded once at the end: the
//! correctly rounded result Db2 for i gives, the same on every host
//! (the platforms' `log1p` differ in the last bit).

/// A value as an unevaluated sum `hi + lo`, `|lo|` at most half an ulp
/// of `hi`.
#[derive(Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
}

/// `ln 2` to about 107 bits.
const LN2: Dd = Dd {
    hi: f64::from_bits(0x3FE6_2E42_FEFA_39EF),
    lo: f64::from_bits(0x3C7A_BC9E_3B39_803F),
};

impl Dd {
    const fn of(x: f64) -> Self {
        Self { hi: x, lo: 0.0 }
    }

    fn add(self, b: Self) -> Self {
        let (sum, err) = two_sum(self.hi, b.hi);
        let (low, low_err) = two_sum(self.lo, b.lo);
        let (sum, err) = fast_two_sum(sum, err + low);
        let (hi, lo) = fast_two_sum(sum, err + low_err);
        Self { hi, lo }
    }

    fn neg(self) -> Self {
        Self {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    fn mul(self, b: Self) -> Self {
        let (p, e) = two_prod(self.hi, b.hi);
        let (hi, lo) = fast_two_sum(p, e + (self.hi * b.lo + self.lo * b.hi));
        Self { hi, lo }
    }

    fn div(self, b: Self) -> Self {
        let q1 = self.hi / b.hi;
        let r = self.add(b.mul(Self::of(q1)).neg());
        let q2 = r.hi / b.hi;
        let r = r.add(b.mul(Self::of(q2)).neg());
        let q3 = r.hi / b.hi;
        let (hi, lo) = fast_two_sum(q1, q2);
        Self { hi, lo }.add(Self::of(q3))
    }

    /// Times a power of two (exact).
    fn scale(self, k: f64) -> Self {
        Self {
            hi: self.hi * k,
            lo: self.lo * k,
        }
    }
}

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn fast_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, b - (s - a))
}

/// Veltkamp's split: no fused multiply-add, so no platform's `fma`.
fn split(a: f64) -> (f64, f64) {
    let t = 134_217_729.0 * a;
    let hi = t - (t - a);
    (hi, a - hi)
}

fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

/// `sum x^k / (2k + 1)` for `k` from 0, `x` at most 1/4.
fn odd_series(x: Dd) -> Dd {
    let mut sum = Dd::of(0.0);
    let mut term = Dd::of(1.0);
    for k in 0..80u32 {
        sum = sum.add(term.div(Dd::of(f64::from(2 * k + 1))));
        term = term.mul(x);
        if term.hi.abs() < sum.hi * 1e-34 {
            break;
        }
    }
    sum
}

/// `ln q` for a positive normal `q`.
fn ln(q: Dd) -> Dd {
    // q = m * 2^e with m in [sqrt(1/2), sqrt(2)).
    let bits = q.hi.to_bits();
    let mut e = i32::try_from((bits >> 52) & 0x7FF).unwrap_or(0) - 1022;
    let mut m = q.scale(pow2(-e));
    if m.hi < std::f64::consts::FRAC_1_SQRT_2 {
        m = m.scale(2.0);
        e -= 1;
    }
    // ln m = 2 atanh(s), s = (m - 1) / (m + 1).
    let s = m.add(Dd::of(-1.0)).div(m.add(Dd::of(1.0)));
    let ln_m = s.mul(odd_series(s.mul(s))).scale(2.0);
    LN2.mul(Dd::of(f64::from(e))).add(ln_m)
}

/// `2^k` for a small `k`.
fn pow2(k: i32) -> f64 {
    let biased = u64::try_from(1023 + k).unwrap_or(1023);
    f64::from_bits(biased << 52)
}

/// The inverse hyperbolic tangent of `x`, `|x| < 1`, correctly rounded.
pub(crate) fn atanh(x: f64) -> f64 {
    let a = x.abs();
    // x^3/3 is under half an ulp of x.
    if a < 7.0e-9 || !a.is_finite() {
        return x;
    }
    let v = if a < 0.5 {
        let d = Dd::of(a);
        d.mul(odd_series(d.mul(d)))
    } else {
        // atanh a = ln((1 + a) / (1 - a)) / 2; 1 - a is exact here.
        let (hi, lo) = two_sum(1.0, a);
        ln(Dd { hi, lo }.div(Dd::of(1.0 - a))).scale(0.5)
    };
    (v.hi + v.lo).copysign(x)
}

#[cfg(test)]
mod tests {
    use super::atanh;

    #[test]
    fn correctly_rounded() {
        // From a 60-digit decimal ln((1 + x) / (1 - x)) / 2.
        for (x, want) in [
            (0.5, 0.549_306_144_334_054_9),
            (-0.5, -0.549_306_144_334_054_9),
            (0.1, 0.100_335_347_731_075_58),
            (0.3, 0.309_519_604_203_111_7),
            (0.4999, 0.549_172_819_888_227_9),
            (0.75, 0.972_955_074_527_656_6),
            (0.9, 1.472_219_489_583_220_4),
            (0.999_999, 7.254_328_619_247_669),
            (1e-10, 1e-10),
            (2.5e-8, 2.500_000_000_000_000_5e-8),
            (0.25, 0.255_412_811_882_995_36),
            (0.6, std::f64::consts::LN_2),
        ] {
            assert_eq!(atanh(x).to_bits(), f64::to_bits(want), "atanh({x})");
        }
    }
}
