// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calling a point's exit programs. A server builds the request in its
//! point's format and calls [`call_exit`] with the registry it can
//! reach (the storage service's, over its link, or one in memory) and
//! the job its programs run in. Every registered program is called in
//! number order with IBM's parameters; the first that refuses ends it.
//! A program that cannot be called or ends in an exception refuses,
//! as IBM documents for the host servers. A point with no programs
//! allows everything.

use crate::error::RegResult;
use crate::formats::ExitRequest;
use crate::model::ExitProgram;

/// Calls a program in the server's job.
pub trait ProgramCaller {
    /// Call `lib/pgm` (`*LIBL` when `lib` is empty) with `parms` by
    /// reference; the program may change the buffers. An error is the
    /// message that kept it from running or ended it.
    fn call(&mut self, pgm: &str, lib: &str, parms: &mut [Vec<u8>]) -> Result<(), String>;
}

/// Where a server finds a point's programs and reports refusals.
pub trait ExitRegistry {
    /// The programs of `point` with `format`, in number order.
    fn programs(&mut self, point: &str, format: &str) -> RegResult<Vec<ExitProgram>>;
    /// A program refused or failed: audited (`GR`) by the registry's
    /// owner.
    fn refused(&mut self, call: &Called) {
        let _ = call;
    }
}

/// One exit program call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Called {
    /// The point and format.
    pub point: String,
    /// Format.
    pub format: String,
    /// Program and library.
    pub pgm: String,
    /// Library.
    pub lib: String,
    /// The return code as one character (`GRRTCD`).
    pub rc: String,
    /// Whether it allowed the request.
    pub allowed: bool,
    /// Why it could not run, when it could not.
    pub failed: Option<String>,
}

ipc::wire_struct!(Called {
    point,
    format,
    pgm,
    lib,
    rc,
    allowed,
    failed
});

/// What the exit programs said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitAnswer {
    /// Whether the request goes ahead.
    pub allow: bool,
    /// The parameters after the last program ran (the request's own
    /// when none did): the outputs a server reads (`tcp::logon_out`,
    /// `telnet::init_out`).
    pub outputs: Vec<Vec<u8>>,
    /// Each program called.
    pub called: Vec<Called>,
    /// Why the request was refused without a program's answer (the
    /// registry could not be read).
    pub error: Option<String>,
}

/// Call the exit programs of `point` for `req`.
pub fn call_exit(
    reg: &mut dyn ExitRegistry,
    caller: &mut dyn ProgramCaller,
    point: &str,
    req: &ExitRequest,
) -> ExitAnswer {
    let format = req.format().to_string();
    let programs = match reg.programs(point, &format) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("exit point {point} {format}: {e}");
            return ExitAnswer {
                allow: false,
                outputs: req.parms(),
                called: Vec::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let mut answer = ExitAnswer {
        allow: true,
        outputs: req.parms(),
        called: Vec::new(),
        error: None,
    };
    for p in programs {
        let lib = if p.lib.is_empty() {
            "*LIBL".to_string()
        } else {
            p.lib.clone()
        };
        let mut parms = req.parms();
        let lens: Vec<usize> = parms.iter().map(Vec::len).collect();
        // A program gets storage behind every parameter, empty or not.
        for b in &mut parms {
            if b.is_empty() {
                b.push(0x40);
            }
        }
        let run = caller.call(&p.pgm, &lib, &mut parms);
        for (b, n) in parms.iter_mut().zip(lens) {
            if n == 0 {
                b.clear();
            }
        }
        let (allowed, failed) = match run {
            Ok(()) => (req.allowed(&parms), None),
            Err(why) => (false, Some(why)),
        };
        let c = Called {
            point: point.to_string(),
            format: format.clone(),
            pgm: p.pgm.clone(),
            lib,
            rc: if failed.is_some() {
                String::new()
            } else {
                req.rc_char(&parms)
            },
            allowed,
            failed,
        };
        answer.outputs = parms;
        if !allowed {
            reg.refused(&c);
            answer.allow = false;
            answer.called.push(c);
            break;
        }
        answer.called.push(c);
    }
    answer
}

/// A job calls exit programs through the runtime's dynamic call.
impl ProgramCaller for mi::Job {
    fn call(&mut self, pgm: &str, lib: &str, parms: &mut [Vec<u8>]) -> Result<(), String> {
        let addrs: Vec<u64> = parms.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
        self.call_program(pgm, lib, &addrs)
            .map_err(|c| format!("{}: {lib}/{pgm} could not be called", c.msgid()))?;
        if let Some(e) = self.clear() {
            return Err(format!("{}: {lib}/{pgm} ended in an exception", e.msgid));
        }
        Ok(())
    }
}
