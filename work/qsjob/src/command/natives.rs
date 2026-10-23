// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command processing programs as programs of `QSYS`: a compiled
//! CL program calls `QCCRTLIB` with the parameter block it built, as
//! IBM's programs call `QLICRLIB`. Each native reads the block at the
//! addresses it was given, runs the command where the inventory says,
//! and writes return values back. `QCLEXEC` is the CL runtime's other
//! path: a command string with placeholders filled from the variables
//! it is given; `QCLSWS` reads the job switches; `QCLDMP` dumps the
//! program.

use std::rc::Rc;

use cmd::analyze::{Env, Setting};
use cmd::block::{self, Parms};
use mi::Job;
use qir::MsgType;
use sys_cpps::Site;

use super::{Fail, RunMode, dispatch_resolved, log, run_exec};
use crate::console::host::Host;

/// Install the natives into a job.
pub fn install(job: &mut Job, host: &Host) {
    for e in sys_cpps::all() {
        let host = host.clone();
        let name = e.name.clone();
        job.natives.insert(
            e.cpp.clone(),
            Rc::new(move |job: &mut Job, args: &[u64]| cpp(job, &host, &name, args)),
        );
    }
    let h = host.clone();
    job.natives.insert(
        "QCLEXEC".into(),
        Rc::new(move |job: &mut Job, args: &[u64]| clexec(job, &h, args)),
    );
    let h = host.clone();
    job.natives.insert(
        "QCLSWS".into(),
        Rc::new(move |job: &mut Job, args: &[u64]| clsws(job, &h, args)),
    );
    job.natives.insert(
        "QCLDMP".into(),
        Rc::new(|job: &mut Job, _args: &[u64]| {
            let text = job.dump(true);
            job.last_dump = Some(text);
        }),
    );
    // The UIM APIs, over the job's display.
    super::uiapi::install(job, host);
}

/// A processing program called with the block's addresses.
fn cpp(job: &mut Job, host: &Host, name: &str, args: &[u64]) {
    let Some((def, lib)) = definition(host, name) else {
        escape(
            job,
            &Fail::new("CPD0030", format!("{name} has no definition")),
        );
        return;
    };
    let mut segments = Vec::with_capacity(def.parms.len());
    for (i, p) in def.parms.iter().enumerate() {
        let addr = args.get(i).copied().unwrap_or(0);
        if addr == 0 {
            segments.push(None);
            continue;
        }
        let n = block::segment_len(p, &mut |off, k| {
            mi::storage::view(addr + off as u64, k).to_vec()
        });
        segments.push(Some(mi::storage::view(addr, n).to_vec()));
    }
    let specified = vec![true; def.parms.len()];
    let parms = match block::read(&def, &segments, &specified) {
        Ok(p) => p,
        Err(e) => {
            escape(job, &Fail::new(e.msgid(), e.to_string()));
            return;
        }
    };
    let parms = mark_specified(host, parms);
    let r = cmd::Resolved {
        name: def.name.clone(),
        lib,
        def: def.clone(),
        label: None,
        prompt: false,
        parms: parms.values.clone(),
    };
    // `LOGCLPGM(*YES)`: the command is a request of the program in the
    // job log, as a typed one would be.
    if host.attrs().logclpgm == "*YES" {
        let (from, _) = job.frame_where(job.act.frames.len().checked_sub(1));
        job.log_request(&r.text(), (&from.0, &from.1));
    }
    let o = dispatch_resolved(job, host, &r);
    for l in o.text.lines() {
        job.console.push(l.to_string());
    }
    if let Some(f) = o.escape {
        escape(job, &f);
        return;
    }
    // Return values: the parameters with `RTNVAL(*YES)` go back into
    // the caller's storage.
    if let Some(back) = o.segments {
        for (i, p) in def.parms.iter().enumerate() {
            if !p.rtnval {
                continue;
            }
            let (Some(addr), Some(Some(seg))) = (args.get(i).copied(), back.get(i)) else {
                continue;
            };
            let held = segments.get(i).and_then(|s| s.as_ref()).map_or(0, Vec::len);
            let n = seg.len().min(held);
            if addr != 0 && n > 0 {
                mi::storage::view(addr, n).copy_from_slice(&seg[..n]);
            }
        }
    }
    let _ = Site::Job;
}

/// The definition of a command, from the job's cache of definitions.
fn definition(host: &Host, name: &str) -> Option<(cmd::CmdDef, String)> {
    let mut cmds = host.cmds.borrow_mut();
    let (o, lib) = cmd::analyze::CmdSource::lookup(&mut *cmds, name, Some("QSYS")).ok()??;
    Some((o.effective(), lib))
}

/// Which parameters the caller gave: those whose value is not the
/// definition's default.
fn mark_specified(host: &Host, mut parms: Parms) -> Parms {
    // The defaults as they come back out of a block, so a default
    // the block cannot hold whole (`*SAME` in one byte) still matches.
    let defaults = {
        let mut cmds = host.cmds.borrow_mut();
        let mut env = Env::new(&mut *cmds);
        env.setting = Setting {
            interactive: host.interactive(),
            program: true,
            ..Setting::default()
        };
        let bounds = cmd::analyze::defaults(&mut env, &parms.def);
        let spec = vec![false; parms.def.parms.len()];
        block::build(&parms.def, &bounds)
            .ok()
            .and_then(|built| block::read(&parms.def, &built.segments, &spec).ok())
            .map_or(bounds, |p| p.values)
    };
    for (i, b) in parms.values.iter_mut().enumerate() {
        let dft = defaults.get(i).and_then(|d| d.value.as_ref());
        b.specified = match (&b.value, dft) {
            (Some(v), Some(d)) => v.source() != d.source(),
            (Some(_), None) => true,
            (None, _) => false,
        };
    }
    parms
}

/// An escape to the caller of the native.
fn escape(job: &mut Job, f: &Fail) {
    log(job, &f.msgid, &f.data, MsgType::Escape, Some(&f.text));
    let _ = job.send_message(
        &f.msgid,
        "QCPFMSG",
        "QSYS",
        f.data.clone(),
        MsgType::Escape,
        mi::QueueRef::Same, // a native has no frame: the caller is on top
    );
}

/// `QCLEXEC(len, template, count, (descriptor, value)...)`: the
/// template's `&n` placeholders take the values, each spelled as a
/// command string spells it, and the string runs.
fn clexec(job: &mut Job, host: &Host, args: &[u64]) {
    let read = |i: usize, n: usize| -> Vec<u8> {
        match args.get(i).copied() {
            Some(a) if a != 0 => mi::storage::view(a, n).to_vec(),
            _ => Vec::new(),
        }
    };
    let packed = |b: &[u8], digits: u8, scale: u8| -> Option<qdecimal::Decimal> {
        qdecimal::Packed::new(digits, scale).ok()?.decode(b).ok()
    };
    let len = packed(&read(0, 6), 10, 0)
        .and_then(|d| d.mantissa_i128())
        .unwrap_or(0);
    let len = usize::try_from(len).unwrap_or(0);
    let template = ccsid::from_ebcdic(&read(1, len));
    let count = packed(&read(2, 6), 10, 0)
        .and_then(|d| d.mantissa_i128())
        .unwrap_or(0);
    let count = usize::try_from(count).unwrap_or(0);
    let mut text = template;
    for k in 0..count {
        let desc = ccsid::from_ebcdic(&read(3 + 2 * k, 16));
        let code = desc.chars().next().unwrap_or('C');
        let width: usize = desc
            .get(1..6)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let scale: u8 = desc
            .get(6..8)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let value = match code {
            'D' => {
                let digits = u8::try_from(width).unwrap_or(15);
                let bytes = read(4 + 2 * k, usize::from(digits) / 2 + 1);
                packed(&bytes, digits, scale).map_or_else(|| "0".into(), |d| d.to_string())
            }
            'L' => {
                let b = read(4 + 2 * k, 1);
                if b.first() == Some(&0xF1) {
                    "'1'".into()
                } else {
                    "'0'".into()
                }
            }
            _ => {
                let s = ccsid::from_ebcdic(&read(4 + 2 * k, width));
                let s = s.trim_end();
                if cmd::value::needs_quotes(s) {
                    format!("'{}'", s.replace('\'', "''"))
                } else {
                    s.to_string()
                }
            }
        };
        text = text.replace(&format!("&{}", k + 1), &value);
    }
    let o = run_exec(job, host, &text, RunMode::Run);
    for l in o.text.lines() {
        job.console.push(l.to_string());
    }
    if let Some(f) = o.escape {
        escape(job, &f);
    }
}

/// `QCLSWS(mask, result)`: whether the job switches match a mask of
/// `0`, `1` and `X`.
fn clsws(_job: &mut Job, host: &Host, args: &[u64]) {
    let (Some(m), Some(r)) = (args.first().copied(), args.get(1).copied()) else {
        return;
    };
    if m == 0 || r == 0 {
        return;
    }
    let mask = ccsid::from_ebcdic(mi::storage::view(m, 8));
    let sws = host.attrs().sws;
    let ok = mask
        .chars()
        .zip(sws.chars().chain(std::iter::repeat('0')))
        .all(|(want, have)| want == 'X' || want == have);
    mi::storage::view(r, 1)[0] = if ok { 0xF1 } else { 0xF0 };
}
