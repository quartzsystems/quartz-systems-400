// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `:`, `true`, `false`, `break`, `continue`, `return`, `exit`.

use super::special_fail;
use crate::msg::Msg;
use crate::shell::{Flow, Res, Shell};

/// `:`.
pub(crate) fn colon(_sh: &mut Shell, _args: &[String]) -> Res {
    Ok(0)
}

/// `true`.
pub(crate) fn true_(_sh: &mut Shell, _args: &[String]) -> Res {
    Ok(0)
}

/// `false`.
pub(crate) fn false_(_sh: &mut Shell, _args: &[String]) -> Res {
    Ok(1)
}

/// The loop count operand of `break` and `continue`.
fn count(sh: &mut Shell, args: &[String]) -> Result<u32, Flow> {
    match args.get(1) {
        None => Ok(1),
        Some(a) => match a.parse::<u32>() {
            Ok(n) if n >= 1 => Ok(n),
            _ => Err(special_fail(
                sh,
                &Msg::NotNumber(args[0].clone(), a.clone()),
                1,
            )),
        },
    }
}

/// `break [n]`.
pub(crate) fn break_(sh: &mut Shell, args: &[String]) -> Res {
    let n = count(sh, args)?;
    if sh.loops == 0 {
        return Ok(0);
    }
    Err(Flow::Break(n.min(sh.loops)))
}

/// `continue [n]`.
pub(crate) fn continue_(sh: &mut Shell, args: &[String]) -> Res {
    let n = count(sh, args)?;
    if sh.loops == 0 {
        return Ok(0);
    }
    Err(Flow::Continue(n.min(sh.loops)))
}

/// A status operand: an integer, taken modulo 256.
fn status_arg(sh: &mut Shell, args: &[String]) -> Result<i32, Flow> {
    match args.get(1) {
        None => Ok(sh.status),
        Some(a) => match a.parse::<i64>() {
            Ok(n) => Ok(i32::try_from(n.rem_euclid(256)).unwrap_or(0)),
            Err(_) => Err(special_fail(
                sh,
                &Msg::NotNumber(args[0].clone(), a.clone()),
                2,
            )),
        },
    }
}

/// `return [n]`.
pub(crate) fn return_(sh: &mut Shell, args: &[String]) -> Res {
    let n = status_arg(sh, args)?;
    if sh.func_depth == 0 && sh.dot_depth == 0 {
        return Err(special_fail(
            sh,
            &Msg::Usage(
                "return".to_string(),
                "not in a function or dot script.".to_string(),
            ),
            1,
        ));
    }
    Err(Flow::Return(n))
}

/// `exit [n]`.
pub(crate) fn exit(sh: &mut Shell, args: &[String]) -> Res {
    let n = status_arg(sh, args)?;
    Err(Flow::Exit(n))
}
