// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ordered-set aggregates: `MEDIAN`, `PERCENTILE_CONT`,
//! `PERCENTILE_DISC` (as `DECFLOAT`, by the values in ascending order,
//! as DB2 for i computes them whatever the direction asked) and
//! `LISTAGG` with its separator and `ON OVERFLOW`.

use std::cmp::Ordering;

use qdecimal::DfFormat;
use qdecimal::decfloat::{add, mul, sub};

use super::{Call, Input};
use crate::ast::Overflow;
use crate::error::{Result, SqlError};
use crate::value::{Value, decfloat_error};

/// The most characters `LISTAGG` gives.
const LISTAGG_MAX: usize = 4000;

pub(super) fn fold(call: &Call<'_>, rows: &[Input]) -> Result<Value> {
    match call.name {
        "LISTAGG" => listagg(call, rows),
        "MEDIAN" => {
            let vals: Vec<Value> = rows.iter().map(|r| r.args[0].clone()).collect();
            percentile(&Value::Str("0.5".into()), vals, true)
        }
        _ => {
            let Some(first) = rows.first() else { return Ok(Value::Null) };
            let p = first.args[0].clone();
            let vals: Vec<Value> =
                rows.iter().filter_map(|r| r.keys.first().filter(|v| !v.is_null()).cloned()).collect();
            percentile(&p, vals, call.name == "PERCENTILE_CONT")
        }
    }
}

fn sorted(mut vals: Vec<Value>) -> Result<Vec<Value>> {
    let mut failure = None;
    vals.sort_by(|a, b| match a.compare(b) {
        Ok(Some(o)) => o,
        Ok(None) => Ordering::Equal,
        Err(e) => {
            failure = Some(e);
            Ordering::Equal
        }
    });
    failure.map_or(Ok(vals), Err)
}

fn percentile(p: &Value, vals: Vec<Value>, cont: bool) -> Result<Value> {
    if vals.is_empty() {
        return Ok(Value::Null);
    }
    let pf = p.as_f64()?;
    if !(0.0..=1.0).contains(&pf) {
        return Err(SqlError::Overflow { what: "a percentile must be from 0 to 1".into() });
    }
    let vals = sorted(vals)?;
    let n = vals.len();
    if !cont {
        // The first value whose cumulative distribution reaches p.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = ((pf * n as f64).ceil() as usize).clamp(1, n) - 1;
        return Ok(vals[i].clone());
    }
    let d34 = |v: &Value| v.as_decfloat(DfFormat::D34);
    let pd = d34(p)?;
    let span = qdecimal::DecFloat::from_i64(i64::try_from(n - 1).expect("fits"), DfFormat::D34);
    let rn = mul(&pd, &span).map_err(decfloat_error)?;
    let lo_i = usize::try_from(rn.to_decimal().map_err(decfloat_error)?.rescale(0, qdecimal::Rounding::Truncate).map_err(decfloat_error)?.to_i128().unwrap_or(0)).unwrap_or(0).min(n - 1);
    let lo = d34(&vals[lo_i])?;
    if lo_i + 1 >= n {
        return Ok(Value::DecFloat(lo));
    }
    let frac = sub(&rn, &qdecimal::DecFloat::from_i64(i64::try_from(lo_i).expect("fits"), DfFormat::D34)).map_err(decfloat_error)?;
    if frac.is_zero() {
        return Ok(Value::DecFloat(lo));
    }
    let hi = d34(&vals[lo_i + 1])?;
    let step = mul(&frac, &sub(&hi, &lo).map_err(decfloat_error)?).map_err(decfloat_error)?;
    Ok(Value::DecFloat(add(&lo, &step).map_err(decfloat_error)?))
}

fn listagg(call: &Call<'_>, rows: &[Input]) -> Result<Value> {
    let sep = rows.first().and_then(|r| r.args.get(1)).filter(|v| !v.is_null()).map(Value::as_text).unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for r in rows {
        if !r.args[0].is_null() {
            parts.push(r.args[0].as_text());
        }
    }
    if parts.is_empty() {
        return Ok(Value::Null);
    }
    let joined = parts.join(&sep);
    if joined.chars().count() <= LISTAGG_MAX {
        return Ok(Value::Str(joined));
    }
    match call.overflow {
        Some(Overflow::Truncate { filler, count }) => {
            let filler = filler.clone().unwrap_or_else(|| "...".into());
            let mut out = String::new();
            let mut used = 0;
            for (i, p) in parts.iter().enumerate() {
                let piece = if i == 0 { p.clone() } else { format!("{sep}{p}") };
                let tail = if *count { format!("{sep}{filler}({})", parts.len() - i) } else { format!("{sep}{filler}") };
                if out.chars().count() + piece.chars().count() + tail.chars().count() > LISTAGG_MAX {
                    out.push_str(&tail);
                    return Ok(Value::Str(out));
                }
                out.push_str(&piece);
                used += 1;
            }
            let _ = used;
            Ok(Value::Str(out))
        }
        _ => Err(SqlError::TooLong { what: format!("LISTAGG result of {} characters", joined.chars().count()) }),
    }
}
