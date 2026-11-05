// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statistical aggregates, as doubles: `STDDEV`, `VARIANCE` and
//! their sample and population forms, `COVARIANCE`, `CORRELATION` and
//! the `REGR_*` regression functions over the pairs with no null.

use qdecimal::{Decimal, Rounding};

use super::Input;
use crate::error::Result;
use crate::value::Value;

/// A double, or null when not finite.
fn double(x: f64) -> Value {
    if x.is_finite() { Value::Double(x) } else { Value::Null }
}

/// The population or sample variance in decimal, kept to 16 significant
/// digits as DB2 for i gives it; `None` when it has no value.
fn variance_decimal(values: &[Value], sample: bool) -> Result<Option<f64>> {
    let n = values.len();
    if n == 0 || (sample && n < 2) {
        return Ok(None);
    }
    let nd = Decimal::new(i128::try_from(n).expect("fits"), 0).expect("fits");
    let ds: Vec<Decimal> = values.iter().map(Value::as_decimal).collect::<Result<_>>()?;
    let mut sum = Decimal::zero();
    for d in &ds {
        sum = sum.add_exact(d).map_err(super::overflow)?;
    }
    let mean = sum.div_to_scale(&nd, 31).map_err(super::overflow)?;
    let mut sq = Decimal::zero();
    for d in &ds {
        let dev = d.sub_exact(&mean).map_err(super::overflow)?;
        let dev = dev.rescale(20.min(dev.scale()), Rounding::Truncate).map_err(super::overflow)?;
        let p = dev.mul_exact(&dev).map_err(super::overflow)?;
        let p = p.rescale(p.scale().min(31), Rounding::Truncate).map_err(super::overflow)?;
        sq = sq.add_exact(&p).map_err(super::overflow)?;
    }
    let div = if sample { Decimal::new(i128::try_from(n - 1).expect("fits"), 0).expect("fits") } else { nd };
    let v = sq.div_to_scale(&div, 31).map_err(super::overflow)?;
    // Sixteen significant digits, the rest dropped.
    let int_digits = v.digits_needed().saturating_sub(v.scale());
    let keep = 16u8.saturating_sub(int_digits.max(1)).min(v.scale());
    let v = v.rescale(keep, Rounding::Truncate).map_err(super::overflow)?;
    Ok(Some(Value::Num(v).as_f64()?))
}

/// The variance in doubles, for the standard deviations.
fn variance_f64(values: &[f64], sample: bool) -> Option<f64> {
    let n = values.len();
    if n == 0 || (sample && n < 2) {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let sq: f64 = values.iter().map(|x| (x - mean) * (x - mean)).sum();
    Some(sq / if sample { nf - 1.0 } else { nf })
}

/// `STDDEV`, `VARIANCE` and their forms over non-null values.
pub(super) fn spread(name: &str, values: &[Value]) -> Result<Value> {
    let sample = matches!(name, "STDDEV_SAMP" | "VAR_SAMP" | "VARIANCE_SAMP");
    if name.starts_with("STDDEV") {
        let xs: Vec<f64> = values.iter().map(Value::as_f64).collect::<Result<_>>()?;
        return Ok(variance_f64(&xs, sample).map_or(Value::Null, |v| double(v.sqrt())));
    }
    Ok(variance_decimal(values, sample)?.map_or(Value::Null, double))
}

/// The two-argument aggregates over pairs (y, x) with no null.
pub(super) fn pairs(name: &str, rows: &[Input]) -> Result<Value> {
    let n = rows.len();
    if name == "REGR_COUNT" {
        return Ok(Value::int(i64::try_from(n).expect("fits")));
    }
    if n == 0 {
        return Ok(Value::Null);
    }
    let mut ys = Vec::with_capacity(n);
    let mut xs = Vec::with_capacity(n);
    for r in rows {
        ys.push(r.args[0].as_f64()?);
        xs.push(r.args[1].as_f64()?);
    }
    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    let (sy, sx): (f64, f64) = (ys.iter().sum(), xs.iter().sum());
    let syy = ys.iter().map(|y| y * y).sum::<f64>() - sy * sy / nf;
    let sxx = xs.iter().map(|x| x * x).sum::<f64>() - sx * sx / nf;
    let (qxx, qyy, qxy) = (
        xs.iter().map(|x| x * x).sum::<f64>(),
        ys.iter().map(|y| y * y).sum::<f64>(),
        ys.iter().zip(&xs).map(|(y, x)| y * x).sum::<f64>(),
    );
    let sxy = qxy - sy * sx / nf;
    // The n-scaled sums, as DB2 for i forms the slope, intercept and correlation.
    let (nxx, nyy, nxy) = (nf * qxx - sx * sx, nf * qyy - sy * sy, nf * qxy - sx * sy);
    let v = match name {
        "COVARIANCE" | "COVAR_POP" | "COVAR" => nxy / (nf * nf),
        "COVARIANCE_SAMP" | "COVAR_SAMP" => {
            if n < 2 {
                return Ok(Value::Null);
            }
            nxy / (nf * (nf - 1.0))
        }
        "CORRELATION" | "CORR" => {
            if nxx == 0.0 || nyy == 0.0 {
                return Ok(Value::Null);
            }
            nxy / (nxx * nyy).sqrt()
        }
        "REGR_AVGX" => sx / nf,
        "REGR_AVGY" => sy / nf,
        "REGR_SXX" => sxx,
        "REGR_SYY" => syy,
        "REGR_SXY" => sxy,
        "REGR_SLOPE" => {
            if nxx == 0.0 {
                return Ok(Value::Null);
            }
            nxy / nxx
        }
        "REGR_INTERCEPT" | "REGR_ICPT" => {
            if sxx == 0.0 {
                return Ok(Value::Null);
            }
            (sy * qxx - sx * qxy) / nxx
        }
        "REGR_R2" => {
            if sxx == 0.0 {
                return Ok(Value::Null);
            }
            if nyy == 0.0 {
                1.0
            } else {
                let r = nxy / (nxx * nyy).sqrt();
                r * r
            }
        }
        _ => return Ok(Value::Null),
    };
    Ok(double(v))
}
